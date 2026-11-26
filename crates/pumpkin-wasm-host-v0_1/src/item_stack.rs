use crate::pumpkin::plugin::attributes::{
    Attribute as WitAttribute, AttributeModifier as WitAttributeModifier,
    ModifierOperation as WitModifierOperation,
};
use crate::pumpkin::plugin::data_components::DataComponent as WitDataComponent;
use crate::pumpkin::plugin::enchantments::Enchantment as WitEnchantment;
use crate::pumpkin::plugin::item_stack::{
    CustomEnchantmentValue as WitCustomEnchantmentValue,
    DataComponentValue as WitDataComponentValue, EnchantmentValue as WitEnchantmentValue,
    Host as ItemStackInterfaceHost, HostItemStack,
    ItemAttributeModifier as WitItemAttributeModifier, ItemStack as ItemStackHandle,
};
use crate::pumpkin::plugin::text::TextComponent as WitTextComponent;
use pumpkin_wasm_host_common::state::PluginHostState;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::Resource;

use super::common::{WitNbtTree, from_wit_nbt_tree, to_wit_nbt_tree};
use pumpkin_data::Enchantment;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::combat::{Modifier, Operation};
use pumpkin_data::data_component_impl::{
    AttributeModifiersImpl, CustomNameImpl, EnchantmentsImpl, LoreImpl, MAX_ENCHANTMENT_LEVEL,
};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_protocol::codec::data_component::{deserialize, serialize};
use std::borrow::Cow;

pub(crate) fn to_wit_data_component(id: DataComponent) -> WitDataComponent {
    // SAFETY: WIT enum is generated in the same order as the internal enum
    unsafe { std::mem::transmute(id as u8) }
}

pub(crate) fn from_wit_data_component(id: WitDataComponent) -> DataComponent {
    // SAFETY: WIT enum is generated in the same order as the internal enum
    unsafe { std::mem::transmute(id as u8) }
}

pub(crate) fn to_wit_enchantment(id: &Enchantment) -> WitEnchantment {
    // SAFETY: WIT enum is generated in the same order as the internal enum
    unsafe { std::mem::transmute(id.id) }
}

pub(crate) fn from_wit_enchantment(id: WitEnchantment) -> &'static Enchantment {
    // Safety: WIT enum is generated in the same order as the internal enum
    Enchantment::from_id(id as u8).expect("valid enchantment ID")
}

#[must_use]
pub fn to_wit_attribute(attr: &Attributes) -> WitAttribute {
    // SAFETY: WIT enum is generated in the same order as the internal ID
    unsafe { std::mem::transmute(attr.id) }
}

#[must_use]
pub const fn from_wit_item_operation(op: WitModifierOperation) -> Operation {
    match op {
        WitModifierOperation::Add => Operation::AddValue,
        WitModifierOperation::MultiplyBase => Operation::AddMultipliedBase,
        WitModifierOperation::MultiplyTotal => Operation::AddMultipliedTotal,
    }
}

#[must_use]
pub const fn to_wit_item_operation(op: Operation) -> WitModifierOperation {
    match op {
        Operation::AddValue => WitModifierOperation::Add,
        Operation::AddMultipliedBase => WitModifierOperation::MultiplyBase,
        Operation::AddMultipliedTotal => WitModifierOperation::MultiplyTotal,
    }
}

impl ItemStackInterfaceHost for PluginHostState {}

impl HostItemStack for PluginHostState {
    fn new(
        &mut self,
        registry_key: String,
        count: u8,
    ) -> wasmtime::Result<Resource<ItemStackHandle>> {
        let item = pumpkin_data::item::Item::from_registry_key(
            registry_key
                .strip_prefix("minecraft:")
                .unwrap_or(&registry_key),
        )
        .unwrap_or(&pumpkin_data::item::Item::AIR);
        let stack = pumpkin_data::item_stack::ItemStack::new(count, item);
        self.add(Arc::new(Mutex::new(stack)))
    }

    async fn get_registry_key(
        &mut self,
        res: Resource<ItemStackHandle>,
    ) -> wasmtime::Result<String> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        Ok(stack.item.registry_key.to_string())
    }

    async fn copy(
        &mut self,
        res: Resource<ItemStackHandle>,
    ) -> wasmtime::Result<Resource<ItemStackHandle>> {
        let cloned = self.get(&res)?.lock().await.clone();
        self.add(Arc::new(Mutex::new(cloned)))
    }

    async fn set_use_remainder(
        &mut self,
        res: Resource<ItemStackHandle>,
        remainder: Option<Resource<ItemStackHandle>>,
    ) -> wasmtime::Result<()> {
        use pumpkin_data::data_component_impl::DataComponentImpl as _;
        let remainder = match remainder {
            Some(remainder) => Some(self.take(remainder)?.lock().await.clone()),
            None => None,
        };
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        let id = DataComponent::UseRemainder;
        stack.patch.retain(|(pid, _)| *pid != id);
        // A patch entry of `None` removes the item's default (e.g. a stew's bowl).
        let value = remainder
            .filter(|remainder| !remainder.is_empty())
            .map(|remainder| {
                pumpkin_data::data_component_impl::UseRemainderImpl {
                    remainder: Some(remainder),
                }
                .to_dyn()
            });
        stack.patch.push((id, value));
        Ok(())
    }

    async fn get_count(&mut self, res: Resource<ItemStackHandle>) -> wasmtime::Result<u8> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        Ok(stack.item_count)
    }

    async fn set_count(
        &mut self,
        res: Resource<ItemStackHandle>,
        count: u8,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        stack.item_count = count;
        Ok(())
    }

    async fn get_max_count(&mut self, res: Resource<ItemStackHandle>) -> wasmtime::Result<u8> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        // Search in components for MaxStackSize
        if let Some((_, data)) = stack
            .item
            .components
            .iter()
            .find(|(id, _)| *id == DataComponent::MaxStackSize)
            && let Some(max_size) = data
                .as_any()
                .downcast_ref::<pumpkin_data::data_component_impl::MaxStackSizeImpl>()
        {
            return Ok(max_size.size);
        }
        Ok(64) // Default
    }

    async fn get_enchantments(
        &mut self,
        res: Resource<ItemStackHandle>,
    ) -> wasmtime::Result<Vec<WitEnchantmentValue>> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        let mut enchantments = Vec::new();
        if let Some((_, Some(data))) = stack
            .patch
            .iter()
            .find(|(id, _)| *id == DataComponent::Enchantments)
            && let Some(enc_impl) = data.as_any().downcast_ref::<EnchantmentsImpl>()
        {
            for (enc, level) in enc_impl.enchantment.iter() {
                enchantments.push(WitEnchantmentValue {
                    enchantment: to_wit_enchantment(enc),
                    level: *level as u32,
                });
            }
        }
        Ok(enchantments)
    }

    async fn add_enchantment(
        &mut self,
        res: Resource<ItemStackHandle>,
        enchantment: WitEnchantment,
        level: u32,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        let enc = from_wit_enchantment(enchantment);

        let mut current_encs = if let Some((_, Some(data))) = stack
            .patch
            .iter()
            .find(|(id, _)| *id == DataComponent::Enchantments)
        {
            data.as_any()
                .downcast_ref::<EnchantmentsImpl>()
                .map(|e| e.enchantment.clone().into_owned())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        current_encs.retain(|(e, _)| e.id != enc.id);
        if level > 0 {
            current_encs.push((enc, level.min(MAX_ENCHANTMENT_LEVEL as u32) as i32));
        }

        if let Some((_, data)) = stack
            .patch
            .iter_mut()
            .find(|(id, _)| *id == DataComponent::Enchantments)
        {
            *data = Some(Box::new(EnchantmentsImpl {
                enchantment: Cow::from(current_encs),
            }));
        } else {
            stack.patch.push((
                DataComponent::Enchantments,
                Some(Box::new(EnchantmentsImpl {
                    enchantment: Cow::from(current_encs),
                })),
            ));
        }
        Ok(())
    }

    async fn remove_enchantment(
        &mut self,
        res: Resource<ItemStackHandle>,
        enchantment: WitEnchantment,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        let enc = from_wit_enchantment(enchantment);

        if let Some((_, Some(data))) = stack
            .patch
            .iter_mut()
            .find(|(id, _)| *id == DataComponent::Enchantments)
            && let Some(enc_impl) = data.as_mut_any().downcast_mut::<EnchantmentsImpl>()
        {
            let mut encs = enc_impl.enchantment.clone().into_owned();
            encs.retain(|(e, _)| e.id != enc.id);
            enc_impl.enchantment = Cow::from(encs);
        }
        Ok(())
    }

    async fn get_custom_enchantments(
        &mut self,
        res: Resource<ItemStackHandle>,
    ) -> wasmtime::Result<Vec<WitCustomEnchantmentValue>> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        let mut result = Vec::new();

        if let Some(compound) = stack.custom_data_compound()
            && let Some(pumpkin_encs) = compound
                .get("pumpkin:enchantments")
                .and_then(NbtTag::extract_compound)
        {
            for (k, v) in &pumpkin_encs.child_tags {
                if let Some(lvl) = v.extract_int() {
                    result.push(WitCustomEnchantmentValue {
                        enchantment_id: k.to_string(),
                        level: (lvl.max(1)) as u32,
                    });
                }
            }
        }

        if let Some((_, Some(data))) = stack
            .patch
            .iter()
            .find(|(id, _)| *id == DataComponent::Enchantments)
            && let Some(enc_impl) = data.as_any().downcast_ref::<EnchantmentsImpl>()
        {
            for (enc, level) in enc_impl.enchantment.iter() {
                if !result.iter().any(|e| e.enchantment_id == enc.name) {
                    result.push(WitCustomEnchantmentValue {
                        enchantment_id: enc.name.to_string(),
                        level: (*level).max(1) as u32,
                    });
                }
            }
        }

        Ok(result)
    }

    async fn add_custom_enchantment(
        &mut self,
        res: Resource<ItemStackHandle>,
        enchantment_id: String,
        level: u32,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;

        if level == 0 {
            stack.remove_custom_data("pumpkin:enchantments", &enchantment_id);
        } else {
            stack.set_custom_data(
                "pumpkin:enchantments",
                &enchantment_id,
                NbtTag::Int(level as i32),
            );
        }

        if let Some(vanilla) = super::enchantment::find_vanilla_enchantment(&enchantment_id) {
            let mut current_encs = if let Some((_, Some(data))) = stack
                .patch
                .iter()
                .find(|(id, _)| *id == DataComponent::Enchantments)
            {
                data.as_any()
                    .downcast_ref::<EnchantmentsImpl>()
                    .map(|e| e.enchantment.clone().into_owned())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            current_encs.retain(|(e, _)| e.id != vanilla.id);
            if level > 0 {
                current_encs.push((vanilla, level.min(MAX_ENCHANTMENT_LEVEL as u32) as i32));
            }

            if let Some((_, data)) = stack
                .patch
                .iter_mut()
                .find(|(id, _)| *id == DataComponent::Enchantments)
            {
                *data = Some(Box::new(EnchantmentsImpl {
                    enchantment: Cow::from(current_encs),
                }));
            } else {
                stack.patch.push((
                    DataComponent::Enchantments,
                    Some(Box::new(EnchantmentsImpl {
                        enchantment: Cow::from(current_encs),
                    })),
                ));
            }
        }

        Ok(())
    }

    async fn remove_custom_enchantment(
        &mut self,
        res: Resource<ItemStackHandle>,
        enchantment_id: String,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;

        stack.remove_custom_data("pumpkin:enchantments", &enchantment_id);

        if let Some(vanilla) = super::enchantment::find_vanilla_enchantment(&enchantment_id)
            && let Some((_, Some(data))) = stack
                .patch
                .iter_mut()
                .find(|(id, _)| *id == DataComponent::Enchantments)
            && let Some(enc_impl) = data.as_mut_any().downcast_mut::<EnchantmentsImpl>()
        {
            let mut encs = enc_impl.enchantment.clone().into_owned();
            encs.retain(|(e, _)| e.id != vanilla.id);
            enc_impl.enchantment = Cow::from(encs);
        }

        Ok(())
    }

    async fn get_custom_enchantment_level(
        &mut self,
        res: Resource<ItemStackHandle>,
        enchantment_id: String,
    ) -> wasmtime::Result<Option<u32>> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;

        if let Some(NbtTag::Int(level)) =
            stack.get_custom_data("pumpkin:enchantments", &enchantment_id)
        {
            return Ok(Some(level.max(1) as u32));
        }

        if let Some(vanilla) = super::enchantment::find_vanilla_enchantment(&enchantment_id)
            && let Some((_, Some(data))) = stack
                .patch
                .iter()
                .find(|(id, _)| *id == DataComponent::Enchantments)
            && let Some(enc_impl) = data.as_any().downcast_ref::<EnchantmentsImpl>()
            && let Some((_, level)) = enc_impl
                .enchantment
                .iter()
                .find(|(e, _)| e.id == vanilla.id)
        {
            return Ok(Some((*level).max(1) as u32));
        }

        Ok(None)
    }

    async fn has_custom_enchantment(
        &mut self,
        res: Resource<ItemStackHandle>,
        enchantment_id: String,
    ) -> wasmtime::Result<bool> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;

        if stack.has_custom_data("pumpkin:enchantments", &enchantment_id) {
            return Ok(true);
        }

        if let Some(vanilla) = super::enchantment::find_vanilla_enchantment(&enchantment_id)
            && let Some((_, Some(data))) = stack
                .patch
                .iter()
                .find(|(id, _)| *id == DataComponent::Enchantments)
            && let Some(enc_impl) = data.as_any().downcast_ref::<EnchantmentsImpl>()
        {
            return Ok(enc_impl.enchantment.iter().any(|(e, _)| e.id == vanilla.id));
        }

        Ok(false)
    }

    async fn get_attribute_modifiers(
        &mut self,
        res: Resource<ItemStackHandle>,
    ) -> wasmtime::Result<Vec<WitItemAttributeModifier>> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        let mut modifiers = Vec::new();
        if let Some(comp) = stack.get_data_component::<AttributeModifiersImpl>() {
            for m in comp.attribute_modifiers.iter() {
                modifiers.push(WitItemAttributeModifier {
                    attribute: to_wit_attribute(m.r#type),
                    modifier: WitAttributeModifier {
                        id: m.id.to_string(),
                        amount: m.amount,
                        operation: to_wit_item_operation(m.operation),
                    },
                    slot: super::enchantment::to_wit_slot(&m.slot),
                });
            }
        }
        Ok(modifiers)
    }

    async fn add_attribute_modifier(
        &mut self,
        res: Resource<ItemStackHandle>,
        modifier: WitItemAttributeModifier,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        let attr = super::living_entity::from_wit_attribute(modifier.attribute);
        let slot = super::enchantment::to_data_slot(modifier.slot);
        let op = from_wit_item_operation(modifier.modifier.operation);
        let mod_id = modifier.modifier.id;

        // Vanilla/Paper: an explicit `attribute_modifiers` component replaces the item's default
        // modifiers instead of adding to them, so start from the stack's own patch only.
        let mut current_mods = explicit_attribute_modifiers(&stack);

        current_mods.retain(|m| !(m.r#type == attr && m.id.as_ref() == mod_id && m.slot == slot));
        current_mods.push(Modifier {
            r#type: attr,
            id: Cow::Owned(mod_id),
            amount: modifier.modifier.amount,
            operation: op,
            slot,
            display: pumpkin_data::data_component_impl::ModifierDisplay::Default,
        });

        stack.set_data_component(AttributeModifiersImpl {
            attribute_modifiers: Cow::Owned(current_mods),
        });

        Ok(())
    }

    async fn remove_attribute_modifiers(
        &mut self,
        res: Resource<ItemStackHandle>,
        attribute: WitAttribute,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        let attr = super::living_entity::from_wit_attribute(attribute);

        // The result is stored explicitly, even when empty, so the item's default modifiers do
        // not come back (vanilla: the component replaces the defaults).
        let mut current_mods = explicit_attribute_modifiers(&stack);
        current_mods.retain(|m| m.r#type != attr);
        stack.set_data_component(AttributeModifiersImpl {
            attribute_modifiers: Cow::Owned(current_mods),
        });

        Ok(())
    }

    async fn clear_attribute_modifiers(
        &mut self,
        res: Resource<ItemStackHandle>,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        stack
            .patch
            .retain(|(id, _)| *id != DataComponent::AttributeModifiers);
        Ok(())
    }

    async fn get_lore(
        &mut self,
        res: Resource<ItemStackHandle>,
    ) -> wasmtime::Result<Vec<Resource<WitTextComponent>>> {
        let stack = self.get(&res)?;
        let lines = {
            let stack = stack.lock().await;
            stack
                .get_data_component::<LoreImpl>()
                .map_or_else(Vec::new, |lore| lore.lines.clone())
        };

        lines.into_iter().map(|line| self.add(line)).collect()
    }

    async fn set_lore(
        &mut self,
        res: Resource<ItemStackHandle>,
        lore: Vec<Resource<WitTextComponent>>,
    ) -> wasmtime::Result<()> {
        let lore = lore
            .into_iter()
            .map(|line| self.take(line))
            .collect::<Result<_, _>>()?;
        let stack = self.get(&res)?;
        stack.lock().await.set_lore(lore);
        Ok(())
    }

    async fn add_lore(
        &mut self,
        res: Resource<ItemStackHandle>,
        line: Resource<WitTextComponent>,
    ) -> wasmtime::Result<()> {
        let line = self.take(line)?;
        let stack = self.get(&res)?;
        stack.lock().await.add_lore(line);
        Ok(())
    }

    async fn get_custom_name(
        &mut self,
        res: Resource<ItemStackHandle>,
    ) -> wasmtime::Result<Option<Resource<WitTextComponent>>> {
        let stack = self.get(&res)?.clone();
        let stack = stack.lock().await;
        if let Some((_, Some(data))) = stack
            .patch
            .iter()
            .find(|(id, _)| *id == DataComponent::CustomName)
            && let Some(name_impl) = data.as_any().downcast_ref::<CustomNameImpl>()
        {
            return Ok(Some(self.add(name_impl.name.clone())?));
        }
        Ok(None)
    }

    async fn set_custom_name(
        &mut self,
        res: Resource<ItemStackHandle>,
        name: Option<Resource<WitTextComponent>>,
    ) -> wasmtime::Result<()> {
        let name = name.map(|r| self.take(r)).transpose()?;
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        if let Some(name) = name {
            if let Some((_, data)) = stack
                .patch
                .iter_mut()
                .find(|(id, _)| *id == DataComponent::CustomName)
            {
                *data = Some(Box::new(CustomNameImpl { name }));
            } else {
                stack.patch.push((
                    DataComponent::CustomName,
                    Some(Box::new(CustomNameImpl { name })),
                ));
            }
        } else {
            stack
                .patch
                .retain(|(id, _)| *id != DataComponent::CustomName);
        }
        Ok(())
    }

    async fn set_custom_data(
        &mut self,
        res: Resource<ItemStackHandle>,
        namespace: String,
        key: String,
        value: WitNbtTree,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        let value = from_wit_nbt_tree(&value).map_err(wasmtime::Error::msg)?;
        stack.set_custom_data(&namespace, &key, value);
        Ok(())
    }

    async fn get_custom_data(
        &mut self,
        res: Resource<ItemStackHandle>,
        namespace: String,
        key: String,
    ) -> wasmtime::Result<Option<WitNbtTree>> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        Ok(stack.get_custom_data(&namespace, &key).map(to_wit_nbt_tree))
    }

    async fn remove_custom_data(
        &mut self,
        res: Resource<ItemStackHandle>,
        namespace: String,
        key: String,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        stack.remove_custom_data(&namespace, &key);
        Ok(())
    }

    async fn has_custom_data(
        &mut self,
        res: Resource<ItemStackHandle>,
        namespace: String,
        key: String,
    ) -> wasmtime::Result<bool> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        Ok(stack.has_custom_data(&namespace, &key))
    }

    async fn get_components(
        &mut self,
        res: Resource<ItemStackHandle>,
    ) -> wasmtime::Result<Vec<WitDataComponentValue>> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        let mut components = Vec::new();
        for (id, data) in &stack.patch {
            if let Some(data) = data {
                let mut buf = Vec::new();
                if serialize(*id, data.as_ref(), &mut buf).is_ok() {
                    components.push(WitDataComponentValue {
                        component: to_wit_data_component(*id),
                        value: buf,
                    });
                }
            }
        }
        Ok(components)
    }

    async fn set_component(
        &mut self,
        res: Resource<ItemStackHandle>,
        component: WitDataComponent,
        value: Vec<u8>,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        let id = from_wit_data_component(component);
        let mut cursor = std::io::Cursor::new(value);

        if let Ok(component_impl) = deserialize(id, &mut cursor) {
            if let Some((_, data)) = stack.patch.iter_mut().find(|(pid, _)| *pid == id) {
                *data = Some(component_impl);
            } else {
                stack.patch.push((id, Some(component_impl)));
            }
        }
        Ok(())
    }

    async fn remove_component(
        &mut self,
        res: Resource<ItemStackHandle>,
        component: WitDataComponent,
    ) -> wasmtime::Result<()> {
        let stack = self.get(&res)?;
        let mut stack = stack.lock().await;
        let id = from_wit_data_component(component);
        stack.patch.retain(|(pid, _)| *pid != id);
        Ok(())
    }

    async fn to_nbt(&mut self, res: Resource<ItemStackHandle>) -> wasmtime::Result<Vec<u8>> {
        let stack = self.get(&res)?;
        let stack = stack.lock().await;
        Ok(stack_to_nbt(&stack))
    }

    fn from_nbt(&mut self, nbt: Vec<u8>) -> wasmtime::Result<Option<Resource<ItemStackHandle>>> {
        stack_from_nbt(&nbt)
            .map(|stack| self.add(Arc::new(Mutex::new(stack))))
            .transpose()
    }

    fn drop(&mut self, rep: Resource<ItemStackHandle>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

/// The whole stack as named Java NBT bytes, as stored in player data.
pub(crate) fn stack_to_nbt(stack: &pumpkin_data::item_stack::ItemStack) -> Vec<u8> {
    // An empty stack has no NBT form; it reads back as `None`, like a missing item.
    let nbt = stack.to_nbt().unwrap_or_default();
    pumpkin_nbt::Nbt::new(String::new(), nbt).write().to_vec()
}

/// Reads bytes made by [`stack_to_nbt`].
pub(crate) fn stack_from_nbt(bytes: &[u8]) -> Option<pumpkin_data::item_stack::ItemStack> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
    let nbt = pumpkin_nbt::Nbt::read(&mut reader).ok()?;
    pumpkin_data::item_stack::ItemStack::read_item_stack(&nbt.root_tag)
}

/// The modifiers in the stack's own `attribute_modifiers` patch, without the item's defaults.
fn explicit_attribute_modifiers(
    stack: &pumpkin_data::item_stack::ItemStack,
) -> Vec<pumpkin_data::data_component_impl::Modifier> {
    stack
        .patch
        .iter()
        .find(|(id, _)| *id == DataComponent::AttributeModifiers)
        .and_then(|(_, component)| component.as_ref())
        .and_then(|component| {
            (**component)
                .as_any()
                .downcast_ref::<AttributeModifiersImpl>()
        })
        .map_or_else(Vec::new, |comp| {
            comp.attribute_modifiers.clone().into_owned()
        })
}
