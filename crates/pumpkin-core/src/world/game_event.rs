use pumpkin_data::game_event::GameEvent;
use pumpkin_util::math::{
    boundingbox::BoundingBox,
    position::{BlockPos, chunk_section_from_pos},
    vector3::Vector3,
};
use pumpkin_world::generation::section_coords::section_to_block;

use super::{World, entity_islands};
use crate::entity::EntityBase;

pub struct GameEventContext<'a> {
    pub source_entity: Option<&'a dyn EntityBase>,
}

pub trait GameEventListener {
    fn listener_position(&self) -> Vector3<f64>;

    fn listener_radius(&self) -> i32;

    fn handle_game_event(
        &self,
        world: &World,
        event: GameEvent,
        context: &GameEventContext<'_>,
        source_position: Vector3<f64>,
    ) -> bool;

    /// Vanilla `DeliveryMode.BY_DISTANCE`: handled after the other listeners, closest first.
    /// Only sculk catalysts use it, so the nearest one eats a death's experience.
    fn delivers_by_distance(&self) -> bool {
        false
    }
}

fn in_range(
    listener: &dyn GameEventListener,
    section_min: Vector3<i32>,
    section_max: Vector3<i32>,
    center: BlockPos,
) -> bool {
    let listener_block = BlockPos::floored_v(listener.listener_position());
    let section = chunk_section_from_pos(&listener_block);
    let listener_radius = listener.listener_radius();
    section.x >= section_min.x
        && section.y >= section_min.y
        && section.z >= section_min.z
        && section.x <= section_max.x
        && section.y <= section_max.y
        && section.z <= section_max.z
        && listener_block.squared_distance(&center) <= listener_radius * listener_radius
}

fn notified_sections(center: BlockPos, radius: i32) -> (Vector3<i32>, Vector3<i32>) {
    (
        chunk_section_from_pos(&center.offset(Vector3::new(-radius, -radius, -radius))),
        chunk_section_from_pos(&center.offset(Vector3::new(radius, radius, radius))),
    )
}

/// The chunk sections whose listeners an event at `position` can notify.
#[must_use]
pub fn notified_area(position: Vector3<f64>, radius: i32) -> BoundingBox {
    let (section_min, section_max) = notified_sections(BlockPos::floored_v(position), radius);
    let block_min = |section: i32| f64::from(section_to_block(section));
    BoundingBox::new(
        Vector3::new(
            block_min(section_min.x),
            block_min(section_min.y),
            block_min(section_min.z),
        ),
        Vector3::new(
            block_min(section_max.x + 1),
            block_min(section_max.y + 1),
            block_min(section_max.z + 1),
        ),
    )
}

impl World {
    pub(super) fn post_game_event(
        &self,
        event: GameEvent,
        position: Vector3<f64>,
        context: &GameEventContext<'_>,
    ) {
        let radius = event.notification_radius();
        let center = BlockPos::floored_v(position);
        let (section_min, section_max) = notified_sections(center, radius);
        let area = notified_area(position, radius);
        for entity in self.entity_index.listeners(self.entities.load_full(), area) {
            let Some(mob) = entity.get_mob() else {
                continue;
            };
            if !self
                .level
                .is_chunk_loaded(&entity.get_entity().chunk_pos.load())
            {
                continue;
            }
            mob.for_each_game_event_listener(&mut |listener| {
                if !in_range(listener, section_min, section_max, center) {
                    return;
                }
                entity_islands::check_reached(entity.get_entity().entity_id);
                listener.handle_game_event(self, event, context, position);
            });
        }

        // Block entity listeners: sculk sensors, shriekers and catalysts.
        let mut block_entities = Vec::new();
        for chunk_x in section_min.x..=section_max.x {
            for chunk_z in section_min.z..=section_max.z {
                if let Some(chunk) = self
                    .block_entities
                    .get(&pumpkin_util::math::vector2::Vector2::new(chunk_x, chunk_z))
                {
                    block_entities.extend(chunk.values().cloned());
                }
            }
        }
        // Vanilla `GameEventDispatcher.post`: BY_DISTANCE listeners are queued and handled after
        // the rest, sorted by their squared distance to the event.
        let mut by_distance = Vec::new();
        for block_entity in block_entities {
            let mut queued = None;
            block_entity.for_each_game_event_listener(&mut |listener| {
                if !in_range(listener, section_min, section_max, center) {
                    return;
                }
                if listener.delivers_by_distance() {
                    let distance = listener
                        .listener_position()
                        .squared_distance_to_vec(&position);
                    queued = Some(queued.map_or(distance, |d: f64| d.min(distance)));
                } else {
                    listener.handle_game_event(self, event, context, position);
                }
            });
            if let Some(distance) = queued {
                by_distance.push((distance, block_entity));
            }
        }
        // Stable, like `Collections.sort`.
        by_distance.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, block_entity) in by_distance {
            block_entity.for_each_game_event_listener(&mut |listener| {
                if listener.delivers_by_distance()
                    && in_range(listener, section_min, section_max, center)
                {
                    listener.handle_game_event(self, event, context, position);
                }
            });
        }
    }
}
