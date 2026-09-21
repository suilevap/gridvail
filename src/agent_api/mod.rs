//! Agent control and observation through the Bevy Remote Protocol (BRP).

use bevy::{
    prelude::*,
    remote::{
        error_codes,
        http::{RemoteHttpPlugin, DEFAULT_PORT},
        BrpError, BrpResult, RemotePlugin,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::*;

pub const STATE_METHOD: &str = "gridvail/state";
pub const MOVE_METHOD: &str = "gridvail/move";
pub const DEFAULT_AGENT_PORT: u16 = DEFAULT_PORT;

const PALETTE_NAMES: [&str; 16] = [
    "black",
    "dark_blue",
    "dark_green",
    "dark_cyan",
    "dark_red",
    "dark_magenta",
    "dark_yellow",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "cyan",
    "red",
    "magenta",
    "yellow",
    "white",
];

/// Loopback HTTP endpoint for agent control. It is installed only when the
/// executable is launched with `--remote` or `--remote-port`.
pub struct AgentApiPlugin {
    port: u16,
}

impl AgentApiPlugin {
    pub const fn new(port: u16) -> Self {
        Self { port }
    }
}

impl Default for AgentApiPlugin {
    fn default() -> Self {
        Self::new(DEFAULT_AGENT_PORT)
    }
}

impl Plugin for AgentApiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
            RemotePlugin::default()
                .with_method_main(STATE_METHOD, get_state)
                .with_method_main(MOVE_METHOD, move_player),
        )
        .add_plugins(RemoteHttpPlugin::default().with_port(self.port));
    }
}

#[derive(Deserialize)]
struct MoveParams {
    direction: MoveDirection,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum MoveDirection {
    #[serde(alias = "U", alias = "north")]
    Up,
    #[serde(alias = "D", alias = "south")]
    Down,
    #[serde(alias = "L", alias = "west")]
    Left,
    #[serde(alias = "R", alias = "east")]
    Right,
    Wait,
}

impl MoveDirection {
    const fn vector(&self) -> IVec2 {
        match self {
            Self::Up => IVec2::NEG_Y,
            Self::Down => IVec2::Y,
            Self::Left => IVec2::NEG_X,
            Self::Right => IVec2::X,
            Self::Wait => IVec2::ZERO,
        }
    }
}

#[derive(Serialize)]
struct Position {
    x: i32,
    y: i32,
}

impl From<IVec2> for Position {
    fn from(position: IVec2) -> Self {
        Self {
            x: position.x,
            y: position.y,
        }
    }
}

#[derive(Serialize)]
struct PlayerState {
    position: Position,
    tokens: i32,
    command_pending: bool,
}

#[derive(Serialize)]
struct VisualState {
    width: i32,
    height: i32,
    /// Final composed glyphs, one string per map row.
    rows: Vec<String>,
    /// Palette indices corresponding one-for-one with `rows` characters.
    colors: Vec<Vec<u8>>,
    palette: &'static [&'static str],
}

#[derive(Serialize)]
struct GameState {
    tick: u64,
    phase: &'static str,
    ready_for_input: bool,
    player: PlayerState,
    enemy_count: usize,
    collision_count: usize,
    visual: VisualState,
}

fn get_state(In(_params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let turn = *world
        .get_resource::<TurnState>()
        .ok_or_else(|| BrpError::resource_not_present("TurnState"))?;
    let collision_count = world
        .get_resource::<CollisionBuffer>()
        .ok_or_else(|| BrpError::resource_not_present("CollisionBuffer"))?
        .0
        .len();

    let (position, tokens, command_pending) = {
        let mut players = world.query_filtered::<(&Pos, &Tokens, &MoveCommand), With<Player>>();
        let (position, tokens, command) = players
            .single(world)
            .map_err(|error| BrpError::internal(format!("player query failed: {error}")))?;
        (position.0, tokens.count, command.active)
    };
    let enemy_count = world
        .query_filtered::<Entity, (With<Enemy>, Without<DestroyRequested>)>()
        .iter(world)
        .count();
    let visual = visual_state(
        world
            .get_resource::<RenderBuffers>()
            .ok_or_else(|| BrpError::resource_not_present("RenderBuffers"))?,
    );

    serialize(GameState {
        tick: turn.tick,
        phase: turn.phase_name(),
        ready_for_input: !turn.simulation && tokens > 0 && !command_pending,
        player: PlayerState {
            position: position.into(),
            tokens,
            command_pending,
        },
        enemy_count,
        collision_count,
        visual,
    })
}

fn move_player(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let params = params.ok_or_else(|| invalid_params("expected {\"direction\":\"up\"}"))?;
    let command: MoveParams =
        serde_json::from_value(params).map_err(|error| invalid_params(error.to_string()))?;
    let turn = *world
        .get_resource::<TurnState>()
        .ok_or_else(|| BrpError::resource_not_present("TurnState"))?;
    if turn.simulation {
        return Ok(json!({ "accepted": false, "reason": "simulation_busy" }));
    }

    let mut players = world.query_filtered::<
        (&mut MoveCommand, &Tokens),
        (With<Player>, With<Active>, Without<DestroyRequested>),
    >();
    let (mut pending, tokens) = players
        .single_mut(world)
        .map_err(|error| BrpError::internal(format!("player query failed: {error}")))?;
    if pending.active {
        return Ok(json!({ "accepted": false, "reason": "command_pending" }));
    }
    if tokens.count <= 0 {
        return Ok(json!({ "accepted": false, "reason": "no_tokens" }));
    }

    pending.target = command.direction.vector();
    pending.relative = true;
    pending.active = true;
    Ok(json!({ "accepted": true }))
}

fn visual_state(buffers: &RenderBuffers) -> VisualState {
    let width = buffers.width as usize;
    let mut rows = Vec::with_capacity(buffers.height as usize);
    let mut colors = Vec::with_capacity(buffers.height as usize);
    for row in buffers.current.chunks(width) {
        rows.push(
            row.iter()
                .map(|cell| match cell.ch {
                    '\0' => ' ',
                    ch => ch,
                })
                .collect(),
        );
        colors.push(row.iter().map(|cell| cell.color).collect());
    }
    VisualState {
        width: buffers.width,
        height: buffers.height,
        rows,
        colors,
        palette: &PALETTE_NAMES,
    }
}

fn serialize(value: impl Serialize) -> BrpResult {
    serde_json::to_value(value).map_err(BrpError::internal)
}

fn invalid_params(message: impl Into<String>) -> BrpError {
    BrpError {
        code: error_codes::INVALID_PARAMS,
        message: message.into(),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::GamePlugin;

    fn game() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins(GamePlugin);
        app.update();
        app
    }

    #[test]
    fn state_contains_the_composed_visual_grid() {
        let mut app = game();
        let value = get_state(In(None), app.world_mut()).unwrap();
        assert_eq!(value["visual"]["width"], 80);
        assert_eq!(value["visual"]["height"], 24);
        assert_eq!(value["visual"]["rows"].as_array().unwrap().len(), 24);
        assert_eq!(value["visual"]["colors"].as_array().unwrap().len(), 24);
        assert_eq!(value["player"]["position"], json!({ "x": 8, "y": 5 }));
        assert_eq!(value["ready_for_input"], true);
    }

    #[test]
    fn remote_move_uses_the_normal_turn_pipeline() {
        let mut app = game();
        let response =
            move_player(In(Some(json!({ "direction": "right" }))), app.world_mut()).unwrap();
        assert_eq!(response, json!({ "accepted": true }));

        app.update();
        let value = get_state(In(None), app.world_mut()).unwrap();
        assert_eq!(value["player"]["position"], json!({ "x": 9, "y": 5 }));
    }

    #[test]
    fn remote_move_rejects_invalid_directions() {
        let mut app = game();
        let error = move_player(
            In(Some(json!({ "direction": "diagonal" }))),
            app.world_mut(),
        )
        .unwrap_err();
        assert_eq!(error.code, error_codes::INVALID_PARAMS);
    }
}
