//! Vanilla `SpellcasterIllager`: spell casting state shared by evokers and illusioners, and the
//! generic `SpellcasterCastingSpellGoal` / `SpellcasterUseSpellGoal`.

use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};

use pumpkin_data::sound::Sound;
use pumpkin_data::tracked_data::spellcaster_illager::SPELL_CASTING_ID;
use pumpkin_nbt::compound::NbtCompound;

use crate::entity::Entity;
use crate::entity::ai::goal::{Controls, Goal};
use crate::entity::mob::Mob;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IllagerSpell {
    None = 0,
    SummonVex = 1,
    Fangs = 2,
    Wololo = 3,
    Disappear = 4,
    Blindness = 5,
}

impl IllagerSpell {
    #[must_use]
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::SummonVex,
            2 => Self::Fangs,
            3 => Self::Wololo,
            4 => Self::Disappear,
            5 => Self::Blindness,
            _ => Self::None,
        }
    }
}

/// `spellCastingTickCount` and `currentSpell`.
pub struct SpellcasterData {
    spell_casting_tick_count: AtomicI32,
    current_spell: AtomicU8,
    casting_sound: Sound,
}

impl SpellcasterData {
    #[must_use]
    pub const fn new(casting_sound: Sound) -> Self {
        Self {
            spell_casting_tick_count: AtomicI32::new(0),
            current_spell: AtomicU8::new(IllagerSpell::None as u8),
            casting_sound,
        }
    }

    #[must_use]
    pub fn is_casting_spell(&self) -> bool {
        self.spell_casting_tick_count.load(Ordering::Relaxed) > 0
    }

    pub fn set_is_casting_spell(&self, entity: &Entity, spell: IllagerSpell) {
        self.current_spell.store(spell as u8, Ordering::Relaxed);
        entity.set_synced_data(SPELL_CASTING_ID, spell as u8 as i8);
    }

    #[must_use]
    pub fn current_spell(&self) -> IllagerSpell {
        IllagerSpell::from_u8(self.current_spell.load(Ordering::Relaxed))
    }

    #[must_use]
    pub fn spell_casting_time(&self) -> i32 {
        self.spell_casting_tick_count.load(Ordering::Relaxed)
    }

    pub fn set_spell_casting_time(&self, ticks: i32) {
        self.spell_casting_tick_count
            .store(ticks, Ordering::Relaxed);
    }

    /// Vanilla `getCastingSoundEvent`.
    #[must_use]
    pub const fn casting_sound(&self) -> Sound {
        self.casting_sound
    }

    /// Vanilla `customServerAiStep`.
    pub fn tick(&self) {
        let ticks = self.spell_casting_tick_count.load(Ordering::Relaxed);
        if ticks > 0 {
            self.spell_casting_tick_count
                .store(ticks - 1, Ordering::Relaxed);
        }
    }

    pub fn write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int("SpellTicks", self.spell_casting_time());
    }

    pub fn read_nbt(&self, nbt: &NbtCompound) {
        self.set_spell_casting_time(nbt.get_int("SpellTicks").unwrap_or(0));
    }
}

/// Vanilla `SpellcasterCastingSpellGoal`: stand still and face the target while a spell is cast.
#[derive(Default)]
pub struct SpellcasterCastingSpellGoal;

impl Goal for SpellcasterCastingSpellGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.spellcaster_data()
            .is_some_and(SpellcasterData::is_casting_spell)
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().steered().navigator().stop();
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(data) = mob.spellcaster_data() {
            data.set_is_casting_spell(mob.get_entity(), IllagerSpell::None);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        if let Some(target) = mob.get_mob_entity().get_target() {
            mob.get_mob_entity()
                .look_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .look_at_entity(mob, &target);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

/// The spell-specific half of vanilla `SpellcasterUseSpellGoal`.
pub trait Spell: Send + Sync {
    /// Extra `canUse` checks after the base ones pass.
    fn can_use(&mut self, _caster: &dyn Mob) -> bool {
        true
    }
    fn on_start(&mut self, _caster: &dyn Mob) {}
    fn cast_warmup_time(&self) -> i32 {
        20
    }
    fn casting_time(&self) -> i32;
    fn casting_interval(&self) -> i32;
    fn prepare_sound(&self) -> Option<Sound>;
    fn spell(&self) -> IllagerSpell;
    fn perform(&mut self, caster: &dyn Mob);
}

/// Vanilla `SpellcasterUseSpellGoal`.
pub struct SpellcasterUseSpellGoal<S: Spell> {
    spell: S,
    attack_warmup_delay: i32,
    next_attack_tick_count: i32,
}

impl<S: Spell> SpellcasterUseSpellGoal<S> {
    #[must_use]
    pub const fn new(spell: S) -> Self {
        Self {
            spell,
            attack_warmup_delay: 0,
            next_attack_tick_count: 0,
        }
    }
}

/// Bukkit `EntitySpellCastEvent`; `false` when a plugin cancelled the cast.
fn spell_cast_allowed(mob: &dyn Mob, spell: IllagerSpell) -> bool {
    let entity = mob.get_entity();
    let mut event = crate::plugin::api::events::entity::entity_spell_cast::EntitySpellCastEvent {
        entity_id: entity.entity_id,
        spell: match spell {
            IllagerSpell::None => "NONE",
            IllagerSpell::SummonVex => "SUMMON_VEX",
            IllagerSpell::Fangs => "FANGS",
            IllagerSpell::Wololo => "WOLOLO",
            IllagerSpell::Disappear => "DISAPPEAR",
            IllagerSpell::Blindness => "BLINDNESS",
        }
        .to_string(),
        cancelled: false,
    };
    !entity.world.load().fire_plugin_event(&mut event)
}

fn has_live_target(mob: &dyn Mob) -> bool {
    mob.get_mob_entity()
        .get_target()
        .is_some_and(|target| target.get_entity().is_alive())
}

impl<S: Spell + 'static> Goal for SpellcasterUseSpellGoal<S> {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(data) = mob.spellcaster_data() else {
            return false;
        };
        has_live_target(mob)
            && !data.is_casting_spell()
            && mob.get_entity().tick_count.load(Ordering::Relaxed) >= self.next_attack_tick_count
            && self.spell.can_use(mob)
            && spell_cast_allowed(mob, self.spell.spell())
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        has_live_target(mob) && self.attack_warmup_delay > 0
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(data) = mob.spellcaster_data() else {
            return;
        };
        self.attack_warmup_delay = self.get_tick_count(self.spell.cast_warmup_time());
        data.set_spell_casting_time(self.spell.casting_time());
        self.next_attack_tick_count =
            mob.get_entity().tick_count.load(Ordering::Relaxed) + self.spell.casting_interval();
        if let Some(sound) = self.spell.prepare_sound() {
            mob.play_entity_sound(sound, 1.0, 1.0);
        }
        data.set_is_casting_spell(mob.get_entity(), self.spell.spell());
        self.spell.on_start(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.attack_warmup_delay -= 1;
        if self.attack_warmup_delay == 0 {
            self.spell.perform(mob);
            if let Some(data) = mob.spellcaster_data() {
                mob.play_entity_sound(data.casting_sound(), 1.0, 1.0);
            }
        }
    }
}
