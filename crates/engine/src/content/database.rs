use std::collections::HashMap;

use crate::app::RenderableKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityDefId(pub u32);

#[derive(Debug, Clone)]
pub struct EntityArchetype {
    pub id: EntityDefId,
    pub def_name: String,
    pub label: String,
    pub renderable: RenderableKind,
    pub move_speed: f32,
    pub health_max: Option<u32>,
    pub base_damage: Option<u32>,
    pub aggro_radius: Option<f32>,
    pub attack_range: Option<f32>,
    pub attack_cooldown_seconds: Option<f32>,
    pub tags: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct DefDatabase {
    entity_defs: Vec<EntityArchetype>,
    entity_ids_by_name: HashMap<String, EntityDefId>,
}

impl DefDatabase {
    pub(crate) fn from_entity_defs(mut entity_defs: Vec<EntityArchetype>) -> Self {
        let mut entity_ids_by_name = HashMap::with_capacity(entity_defs.len());
        for (idx, def) in entity_defs.iter_mut().enumerate() {
            let id = EntityDefId(idx as u32);
            def.id = id;
            entity_ids_by_name.insert(def.def_name.clone(), id);
        }
        Self {
            entity_defs,
            entity_ids_by_name,
        }
    }

    pub fn entity_def_id_by_name(&self, name: &str) -> Option<EntityDefId> {
        self.entity_ids_by_name.get(name).copied()
    }

    pub fn entity_def(&self, id: EntityDefId) -> Option<&EntityArchetype> {
        self.entity_defs.get(id.0 as usize)
    }

    pub fn entity_defs(&self) -> &[EntityArchetype] {
        &self.entity_defs
    }
}
