fn build_ground_tilemap(scene_key: SceneKey) -> Tilemap {
    let width = 16u32;
    let height = 12u32;
    let mut tiles = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let checker = ((x + y) % 2) as u16;
            let tile_id = match scene_key {
                SceneKey::A => checker,
                SceneKey::B => 1u16.saturating_sub(checker),
            };
            tiles.push(tile_id);
        }
    }
    Tilemap::new(
        width,
        height,
        Vec2 {
            x: -(width as f32) / 2.0,
            y: -(height as f32) / 2.0,
        },
        tiles,
    )
    .expect("static tilemap shape is valid")
}

fn try_resolve_archetype_by_name(
    world: &SceneWorld,
    def_name: &str,
) -> SaveLoadResult<EntityArchetype> {
    let def_db = world
        .def_database()
        .ok_or_else(|| "DefDatabase not set on SceneWorld before scene load".to_string())?;
    let def_id = def_db
        .entity_def_id_by_name(def_name)
        .ok_or_else(|| format!("unknown entity def '{def_name}'"))?;
    def_db
        .entity_def(def_id)
        .ok_or_else(|| format!("EntityDef id for '{def_name}' is missing from DefDatabase"))
        .cloned()
}

fn resolve_player_archetype(world: &SceneWorld) -> EntityArchetype {
    try_resolve_player_archetype(world).unwrap_or_else(|error| panic!("{error}"))
}

fn try_resolve_player_archetype(world: &SceneWorld) -> SaveLoadResult<EntityArchetype> {
    let def_db = world
        .def_database()
        .ok_or_else(|| "DefDatabase not set on SceneWorld before scene load".to_string())?;
    let player_id = def_db
        .entity_def_id_by_name("proto.player")
        .ok_or_else(|| {
            "missing EntityDef 'proto.player'; add it to assets/base or enabled mods and fix XML compile errors"
                .to_string()
        })?;
    def_db
        .entity_def(player_id)
        .ok_or_else(|| "EntityDef id for 'proto.player' is missing from DefDatabase".to_string())
        .cloned()
}

fn resolve_resource_pile_archetype(world: &SceneWorld) -> EntityArchetype {
    try_resolve_resource_pile_archetype(world).unwrap_or_else(|error| panic!("{error}"))
}

fn try_resolve_resource_pile_archetype(world: &SceneWorld) -> SaveLoadResult<EntityArchetype> {
    let def_db = world
        .def_database()
        .ok_or_else(|| "DefDatabase not set on SceneWorld before scene load".to_string())?;
    let pile_id = def_db
        .entity_def_id_by_name("proto.resource_pile")
        .ok_or_else(|| {
            "missing EntityDef 'proto.resource_pile'; add it to assets/base or enabled mods and fix XML compile errors"
                .to_string()
        })?;
    let pile = def_db
        .entity_def(pile_id)
        .ok_or_else(|| {
            "EntityDef id for 'proto.resource_pile' is missing from DefDatabase".to_string()
        })?
        .clone();
    let has_interactable_tag = pile.tags.iter().any(|tag| tag == "interactable");
    let has_resource_pile_tag = pile.tags.iter().any(|tag| tag == "resource_pile");
    if !(has_interactable_tag && has_resource_pile_tag) {
        return Err(
            "EntityDef 'proto.resource_pile' must include tags 'interactable' and 'resource_pile'"
                .to_string(),
        );
    }
    Ok(pile)
}

fn movement_delta(input: &InputSnapshot, fixed_dt_seconds: f32, speed: f32) -> Vec2 {
    let mut x = 0.0f32;
    let mut y = 0.0f32;

    if input.is_down(InputAction::MoveRight) {
        x += 1.0;
    }
    if input.is_down(InputAction::MoveLeft) {
        x -= 1.0;
    }
    if input.is_down(InputAction::MoveUp) {
        y += 1.0;
    }
    if input.is_down(InputAction::MoveDown) {
        y -= 1.0;
    }

    let len_sq = x * x + y * y;
    if len_sq > 0.0 {
        let inv_len = len_sq.sqrt().recip();
        x *= inv_len;
        y *= inv_len;
    }

    Vec2 {
        x: x * speed * fixed_dt_seconds,
        y: y * speed * fixed_dt_seconds,
    }
}

fn camera_delta(input: &InputSnapshot, fixed_dt_seconds: f32, speed: f32) -> Vec2 {
    let mut x = 0.0f32;
    let mut y = 0.0f32;

    if input.is_down(InputAction::CameraRight) {
        x += 1.0;
    }
    if input.is_down(InputAction::CameraLeft) {
        x -= 1.0;
    }
    if input.is_down(InputAction::CameraUp) {
        y += 1.0;
    }
    if input.is_down(InputAction::CameraDown) {
        y -= 1.0;
    }

    let len_sq = x * x + y * y;
    if len_sq > 0.0 {
        let inv_len = len_sq.sqrt().recip();
        x *= inv_len;
        y *= inv_len;
    }

    Vec2 {
        x: x * speed * fixed_dt_seconds,
        y: y * speed * fixed_dt_seconds,
    }
}

fn step_toward(
    current: Vec2,
    target: Vec2,
    speed: f32,
    fixed_dt_seconds: f32,
    arrival_threshold: f32,
) -> (Vec2, bool) {
    let dx = target.x - current.x;
    let dy = target.y - current.y;
    let distance_sq = dx * dx + dy * dy;
    let threshold_sq = arrival_threshold * arrival_threshold;
    if distance_sq <= threshold_sq {
        return (target, true);
    }

    let distance = distance_sq.sqrt();
    let max_step = speed * fixed_dt_seconds;
    if max_step >= distance {
        return (target, true);
    }

    let inv_distance = distance.recip();
    (
        Vec2 {
            x: current.x + dx * inv_distance * max_step,
            y: current.y + dy * inv_distance * max_step,
        },
        false,
    )
}
