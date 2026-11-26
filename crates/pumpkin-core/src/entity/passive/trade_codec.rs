//! Trade recipe pieces shared by villagers and wandering traders (`MerchantOffer` NBT and the
//! Bedrock trade table).

use pumpkin_data::item::JavaToBedrockItemMapping;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::compound::NbtCompound;

/// A Bedrock trade item entry: `Count`, `Damage`, `Name`; empty for an empty or unmapped stack.
#[must_use]
pub fn bedrock_trade_item(stack: &ItemStack, count: u8) -> NbtCompound {
    let mut item = NbtCompound::new();
    if stack.is_empty() {
        return item;
    }
    let Some(mapping) = JavaToBedrockItemMapping::from_java_item_id(stack.item.id) else {
        return item;
    };
    item.put_byte("Count", count as i8);
    item.put_short("Damage", mapping.bedrock_data as i16);
    item.put_string("Name", mapping.bedrock_item.registry_key.to_owned());
    item
}

/// Bedrock always sends `buyB`, an empty compound for "no second cost".
#[must_use]
pub fn bedrock_buy_b(cost_b: Option<&ItemStack>) -> NbtCompound {
    cost_b.map_or_else(NbtCompound::new, |cost| {
        bedrock_trade_item(cost, cost.item_count)
    })
}

/// `buyB` for the saved offer: `Some(None)` without a second cost, `None` when the cost can't
/// be saved (the offer is skipped).
#[must_use]
pub fn saved_buy_b(cost_b: Option<&ItemStack>) -> Option<Option<NbtCompound>> {
    cost_b.map_or(Some(None), |cost| cost.to_nbt().map(Some))
}

/// `buyB` read back: a present but invalid entry rejects the recipe (`None`), like a failed
/// vanilla codec decode.
#[must_use]
pub fn read_buy_b(recipe: &NbtCompound) -> Option<Option<ItemStack>> {
    recipe.get("buyB").map_or(Some(None), |cost_b| {
        cost_b
            .extract_compound()
            .and_then(ItemStack::read_item_stack)
            .map(Some)
    })
}
