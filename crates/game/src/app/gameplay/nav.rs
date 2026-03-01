const FNV1A_OFFSET_BASIS_64: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME_64: u64 = 0x0000_0100_0000_01b3;
const NAV_BLOCKED_TILE_ID: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TileCoord {
    x: u32,
    y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TilemapNavKey {
    width: u32,
    height: u32,
    origin_x_bits: u32,
    origin_y_bits: u32,
    tiles_hash: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct NavigationPathState {
    goal_tile: TileCoord,
    waypoints_world: Vec<Vec2>,
    next_waypoint_index: usize,
}

impl NavigationPathState {
    fn current_waypoint(&self) -> Option<Vec2> {
        self.waypoints_world.get(self.next_waypoint_index).copied()
    }

    fn advance_waypoint(&mut self) {
        if self.next_waypoint_index < self.waypoints_world.len() {
            self.next_waypoint_index = self.next_waypoint_index.saturating_add(1);
        }
    }

    fn is_complete(&self) -> bool {
        self.next_waypoint_index >= self.waypoints_world.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct NavigationPassabilityCache {
    key: Option<TilemapNavKey>,
    width: u32,
    height: u32,
    origin: Vec2,
    walkable: Vec<bool>,
}

impl Default for NavigationPassabilityCache {
    fn default() -> Self {
        Self {
            key: None,
            width: 0,
            height: 0,
            origin: Vec2 { x: 0.0, y: 0.0 },
            walkable: Vec::new(),
        }
    }
}

impl NavigationPassabilityCache {
    fn clear(&mut self) {
        self.key = None;
        self.width = 0;
        self.height = 0;
        self.origin = Vec2 { x: 0.0, y: 0.0 };
        self.walkable.clear();
    }

    fn refresh_from_tilemap(&mut self, tilemap: Option<&Tilemap>) {
        let Some(tilemap) = tilemap else {
            self.clear();
            return;
        };
        let key = compute_tilemap_nav_key(tilemap);
        if self.key == Some(key) {
            return;
        }

        let width = tilemap.width();
        let height = tilemap.height();
        let mut walkable = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let tile_id = tilemap.tile_at(x, y).unwrap_or(0);
                walkable.push(tile_id != NAV_BLOCKED_TILE_ID);
            }
        }

        self.key = Some(key);
        self.width = width;
        self.height = height;
        self.origin = tilemap.origin();
        self.walkable = walkable;
    }

    fn world_to_tile(&self, world: Vec2) -> Option<TileCoord> {
        self.key?;
        let tile_x = (world.x - self.origin.x).floor() as i32;
        let tile_y = (world.y - self.origin.y).floor() as i32;
        if tile_x < 0 || tile_y < 0 {
            return None;
        }
        let tile_x = tile_x as u32;
        let tile_y = tile_y as u32;
        if tile_x >= self.width || tile_y >= self.height {
            return None;
        }
        Some(TileCoord {
            x: tile_x,
            y: tile_y,
        })
    }

    fn tile_center_world(&self, tile: TileCoord) -> Vec2 {
        Vec2 {
            x: self.origin.x + tile.x as f32 + 0.5,
            y: self.origin.y + tile.y as f32 + 0.5,
        }
    }

    fn build_path_state_from_world(
        &self,
        start_world: Vec2,
        goal_world: Vec2,
    ) -> Option<NavigationPathState> {
        let start_tile = self.world_to_tile(start_world)?;
        let goal_tile = self.world_to_tile(goal_world)?;
        if !self.is_walkable(start_tile) || !self.is_walkable(goal_tile) {
            return None;
        }

        let tile_path = self.find_path_tiles(start_tile, goal_tile)?;
        if tile_path.is_empty() {
            return None;
        }

        let mut waypoints_world = Vec::new();
        if tile_path.len() == 1 {
            waypoints_world.push(self.tile_center_world(goal_tile));
        } else {
            for tile in tile_path.iter().skip(1) {
                waypoints_world.push(self.tile_center_world(*tile));
            }
        }

        Some(NavigationPathState {
            goal_tile,
            waypoints_world,
            next_waypoint_index: 0,
        })
    }

    fn is_walkable(&self, tile: TileCoord) -> bool {
        self.index_of(tile)
            .and_then(|index| self.walkable.get(index))
            .copied()
            .unwrap_or(false)
    }

    fn index_of(&self, tile: TileCoord) -> Option<usize> {
        if tile.x >= self.width || tile.y >= self.height {
            return None;
        }
        Some(tile.y as usize * self.width as usize + tile.x as usize)
    }

    fn find_path_tiles(&self, start: TileCoord, goal: TileCoord) -> Option<Vec<TileCoord>> {
        let start_index = self.index_of(start)?;
        let goal_index = self.index_of(goal)?;
        if !self.is_walkable(start) || !self.is_walkable(goal) {
            return None;
        }

        if start == goal {
            return Some(vec![start]);
        }

        let node_count = (self.width * self.height) as usize;
        let mut closed = vec![false; node_count];
        let mut best_g = vec![u32::MAX; node_count];
        let mut parent = vec![None::<usize>; node_count];
        let mut open = Vec::new();
        let mut next_insertion = 0u64;

        let start_h = manhattan_distance(start, goal);
        open.push(OpenNode {
            coord: start,
            h_cost: start_h,
            f_cost: start_h,
            insertion_order: next_insertion,
        });
        next_insertion = next_insertion.saturating_add(1);
        best_g[start_index] = 0;

        while !open.is_empty() {
            let best_index = pick_best_open_node_index(&open);
            let current = open.swap_remove(best_index);
            let Some(current_index) = self.index_of(current.coord) else {
                continue;
            };
            if closed[current_index] {
                continue;
            }
            closed[current_index] = true;

            if current.coord == goal {
                return reconstruct_tile_path(&parent, self.width, start_index, goal_index);
            }

            let current_g = best_g[current_index];
            for neighbor in self.neighbors(current.coord) {
                let Some(neighbor) = neighbor else {
                    continue;
                };
                let Some(neighbor_index) = self.index_of(neighbor) else {
                    continue;
                };
                if closed[neighbor_index] || !self.is_walkable(neighbor) {
                    continue;
                }

                let tentative_g = current_g.saturating_add(1);
                if tentative_g >= best_g[neighbor_index] {
                    continue;
                }

                best_g[neighbor_index] = tentative_g;
                parent[neighbor_index] = Some(current_index);
                let h_cost = manhattan_distance(neighbor, goal);
                open.push(OpenNode {
                    coord: neighbor,
                    h_cost,
                    f_cost: tentative_g.saturating_add(h_cost),
                    insertion_order: next_insertion,
                });
                next_insertion = next_insertion.saturating_add(1);
            }
        }

        None
    }

    fn neighbors(&self, coord: TileCoord) -> [Option<TileCoord>; 4] {
        let north = if coord.y < self.height.saturating_sub(1) {
            Some(TileCoord {
                x: coord.x,
                y: coord.y + 1,
            })
        } else {
            None
        };
        let east = if coord.x < self.width.saturating_sub(1) {
            Some(TileCoord {
                x: coord.x + 1,
                y: coord.y,
            })
        } else {
            None
        };
        let south = if coord.y > 0 {
            Some(TileCoord {
                x: coord.x,
                y: coord.y - 1,
            })
        } else {
            None
        };
        let west = if coord.x > 0 {
            Some(TileCoord {
                x: coord.x - 1,
                y: coord.y,
            })
        } else {
            None
        };
        [north, east, south, west]
    }
}

#[derive(Debug, Clone, Copy)]
struct OpenNode {
    coord: TileCoord,
    h_cost: u32,
    f_cost: u32,
    insertion_order: u64,
}

fn pick_best_open_node_index(open: &[OpenNode]) -> usize {
    let mut best_index = 0usize;
    for index in 1..open.len() {
        let current = open[index];
        let best = open[best_index];
        if open_node_order_key(current) < open_node_order_key(best) {
            best_index = index;
        }
    }
    best_index
}

fn open_node_order_key(node: OpenNode) -> (u32, u32, u32, u32, u64) {
    (
        node.f_cost,
        node.h_cost,
        node.coord.y,
        node.coord.x,
        node.insertion_order,
    )
}

fn reconstruct_tile_path(
    parent: &[Option<usize>],
    width: u32,
    start_index: usize,
    goal_index: usize,
) -> Option<Vec<TileCoord>> {
    let mut cursor = goal_index;
    let mut indices = vec![cursor];

    while cursor != start_index {
        let next = parent.get(cursor).and_then(|value| *value)?;
        cursor = next;
        indices.push(cursor);
    }
    indices.reverse();
    Some(
        indices
            .into_iter()
            .map(|index| TileCoord {
                x: (index as u32) % width,
                y: (index as u32) / width,
            })
            .collect(),
    )
}

fn manhattan_distance(a: TileCoord, b: TileCoord) -> u32 {
    a.x.abs_diff(b.x).saturating_add(a.y.abs_diff(b.y))
}

fn compute_tilemap_nav_key(tilemap: &Tilemap) -> TilemapNavKey {
    let mut tiles_hash = FNV1A_OFFSET_BASIS_64;
    for y in 0..tilemap.height() {
        for x in 0..tilemap.width() {
            let tile_id = tilemap.tile_at(x, y).unwrap_or(0);
            tiles_hash = fnv1a_update_u16(tiles_hash, tile_id);
        }
    }
    TilemapNavKey {
        width: tilemap.width(),
        height: tilemap.height(),
        origin_x_bits: tilemap.origin().x.to_bits(),
        origin_y_bits: tilemap.origin().y.to_bits(),
        tiles_hash,
    }
}

fn fnv1a_update_u16(mut hash: u64, value: u16) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV1A_PRIME_64);
    }
    hash
}

#[cfg(test)]
mod nav_tests {
    use super::*;

    fn cache_from_tilemap(tilemap: Tilemap) -> NavigationPassabilityCache {
        let mut cache = NavigationPassabilityCache::default();
        cache.refresh_from_tilemap(Some(&tilemap));
        cache
    }

    fn tilemap_with_tiles(width: u32, height: u32, tiles: Vec<u16>) -> Tilemap {
        Tilemap::new(width, height, Vec2 { x: 0.0, y: 0.0 }, tiles).expect("tilemap")
    }

    #[test]
    fn astar_path_never_steps_onto_blocked_tile_id() {
        let width = 7u32;
        let height = 5u32;
        let mut tiles = vec![0u16; (width * height) as usize];
        for y in 0..height {
            if y != 4 {
                let index = (y * width + 3) as usize;
                tiles[index] = NAV_BLOCKED_TILE_ID;
            }
        }
        let cache = cache_from_tilemap(tilemap_with_tiles(width, height, tiles));

        let start = cache.tile_center_world(TileCoord { x: 1, y: 2 });
        let goal = cache.tile_center_world(TileCoord { x: 5, y: 2 });
        let path = cache
            .build_path_state_from_world(start, goal)
            .expect("expected reachable path");
        assert!(!path.waypoints_world.is_empty());
        for waypoint in path.waypoints_world {
            let tile = cache.world_to_tile(waypoint).expect("waypoint tile");
            assert!(cache.is_walkable(tile), "waypoint stepped onto blocked tile");
        }
    }

    #[test]
    fn astar_tie_break_is_deterministic_on_symmetric_map() {
        let width = 5u32;
        let height = 5u32;
        let mut tiles = vec![0u16; (width * height) as usize];
        tiles[(2 * width + 2) as usize] = NAV_BLOCKED_TILE_ID;
        let cache = cache_from_tilemap(tilemap_with_tiles(width, height, tiles));

        let start = cache.tile_center_world(TileCoord { x: 0, y: 2 });
        let goal = cache.tile_center_world(TileCoord { x: 4, y: 2 });
        let first = cache
            .build_path_state_from_world(start, goal)
            .expect("first path");
        let second = cache
            .build_path_state_from_world(start, goal)
            .expect("second path");
        assert_eq!(first, second);
    }
}
