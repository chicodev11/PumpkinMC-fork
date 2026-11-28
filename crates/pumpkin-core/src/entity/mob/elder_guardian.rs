use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use pumpkin_data::effect::StatusEffect;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::Sound;
use pumpkin_protocol::java::client::play::{CGameEvent, GameEvent};

use crate::entity::custom_sound::CustomSound;
use crate::entity::mob::guardian::GuardianCore;
use crate::entity::{Entity, EntityBase, mob::Mob};

const EFFECT_INTERVAL: i32 = 1200;
const EFFECT_RADIUS: f64 = 50.0;
const EFFECT_DURATION: i32 = 6000;
const EFFECT_AMPLIFIER: u8 = 2;
/// Players who still have this much mining fatigue left are not cursed again.
const EFFECT_DISPLAY_LIMIT: i32 = 1200;

pub struct ElderGuardianEntity {
    pub core: GuardianCore,
}

impl ElderGuardianEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_arc = Arc::new(Self {
            core: GuardianCore::new(entity, true),
        });
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };
        mob_arc.core.register_goals(mob_weak);
        mob_arc
    }
}

impl CustomSound for ElderGuardianEntity {
    fn hurt_sound(&self) -> Option<Sound> {
        Some(if self.get_entity().is_in_water() {
            Sound::EntityElderGuardianHurt
        } else {
            Sound::EntityElderGuardianHurtLand
        })
    }

    fn death_sound(&self) -> Option<Sound> {
        Some(if self.get_entity().is_in_water() {
            Sound::EntityElderGuardianDeath
        } else {
            Sound::EntityElderGuardianDeathLand
        })
    }
}

impl Mob for ElderGuardianEntity {
    crate::guardian_mob_methods!();

    fn get_ambient_sound(&self) -> Option<Sound> {
        Some(if self.get_entity().is_in_water() {
            Sound::EntityElderGuardianAmbient
        } else {
            Sound::EntityElderGuardianAmbientLand
        })
    }

    /// Vanilla `ElderGuardian.customServerAiStep`: curses nearby players with mining fatigue
    /// every minute and stays near where it first stood.
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        let mob_entity = &self.core.mob_entity;
        let entity = &mob_entity.living_entity.entity;
        if (entity.tick_count.load(Ordering::Relaxed) + entity.entity_id) % EFFECT_INTERVAL == 0 {
            let pos = entity.pos.load();
            let world = entity.world.load();
            let packet = CGameEvent::new(
                GameEvent::PlayElderGuardianMobAppearance,
                if entity.is_silent() { 0.0 } else { 1.0 },
            );
            for player in world.get_nearby_players(pos, EFFECT_RADIUS) {
                // Vanilla `MobEffectUtil.addEffectToPlayersAround`
                if !matches!(
                    player.gamemode.load(),
                    pumpkin_util::GameMode::Survival | pumpkin_util::GameMode::Adventure
                ) || player
                    .living_entity
                    .get_effect(&StatusEffect::MINING_FATIGUE)
                    .is_some_and(|effect| {
                        effect.amplifier >= EFFECT_AMPLIFIER
                            && effect.duration >= EFFECT_DISPLAY_LIMIT
                    })
                {
                    continue;
                }
                // Paper `ElderGuardianAppearanceEvent`
                if player
                    .plugin_event(|affected_player| {
                        crate::plugin::api::events::entity::elder_guardian_appearance::ElderGuardianAppearanceEvent {
                            guardian: world
                                .get_entity_by_id(entity.entity_id)
                                .unwrap_or_else(|| affected_player.clone()),
                            affected_player,
                            cancelled: false,
                        }
                    })
                    .is_some_and(|event| event.cancelled)
                {
                    continue;
                }
                player.try_send_client_packet(&packet);
                player.living_entity.add_effect(Effect {
                    effect_type: &StatusEffect::MINING_FATIGUE,
                    duration: EFFECT_DURATION,
                    amplifier: EFFECT_AMPLIFIER,
                    ambient: false,
                    show_particles: true,
                    show_icon: true,
                    blend: false,
                });
            }
        }
        if mob_entity.position_target_range.load(Ordering::Relaxed) < 0 {
            mob_entity.set_home_to(entity.block_pos.load(), 16);
        }
    }
}
