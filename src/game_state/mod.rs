use glium::{DrawParameters, DepthTest, Depth, Blend};
use glium::uniforms::{Sampler, MinifySamplerFilter, MagnifySamplerFilter, SamplerWrapFunction};
use specs::{World, Planner, Join, Gate};

use assets::{get_asset_string, get_asset_bytes};
use game::Game;
use rendering::*;
use state::*;
use systems::*;
use vectors::*;

pub struct GameState
{
    shader: Shader,
    mesh: Mesh,
    atlas: TextureAtlas,
    planner: Planner<()>,
    camera_pos: Vector2<f32>,
    time: f64
}

impl State for GameState
{
    fn new(display: &Display, game: &mut Game) -> Self
    {
        let shader = load_shader(display, &get_asset_string("shaders/sprite.vs"), &get_asset_string("shaders/sprite.fs"));
        let mesh = quad_mesh(display);
        let atlas = load_texture_atlas(display, &get_asset_bytes("atlas.png"), 16);

        let mut world = World::new();
        world.register::<Position>();
        world.register::<Sprite>();
        world.register::<Motion>();
        world.register::<Player>();
        world.register::<Collision>();
        world.register::<Hazard>();
        world.register::<Goal>();
        world.register::<PlayerTracker>();
        world.register::<Button>();
        world.register::<ButtonGate>();

        let camera_pos: Vector2<f32>;
        {
            let level = &game.levels[game.current_level];
            camera_pos = level.midpoint;

            world.create_now()
                .with(Position(level.player_pos))
                .with(Sprite { region: vec2(0, 0), layer: visual::ACTOR_LAYER })
                .with(Motion::new(4.0))
                .with(Player::default())
                .build();

            world.create_now()
                .with(Position(level.stalker_pos))
                .with(Sprite { region: vec2(0, 1), layer: visual::ACTOR_LAYER })
                .with(Motion::new(4.0))
                .with(Collision::BlocksPush)
                .with(Hazard)
                .with(PlayerTracker::new(0.05, level.initial_stalker_path.clone()))
                .build();

            for door in &level.doors
            {
                world.create_now()
                    .with(Position(*door))
                    .with(Sprite { region: vec2(1, 2), layer: visual::BG_LAYER })
                    .build();

                world.create_now()
                    .with(Position(*door))
                    .with(Sprite { region: vec2(0, 3), layer: visual::OBJECT_LAYER })
                    .with(Collision::BlocksPush)
                    .with(Goal)
                    .build();
            }

            for &(style, pos) in &level.blocks
            {
                world.create_now()
                    .with(Position(pos))
                    .with(Sprite { region: vec2(style, 2), layer: visual::BG_LAYER })
                    .with(Collision::Obstacle)
                    .build();
            }

            for push_block in &level.push_blocks
            {
                world.create_now()
                    .with(Position(*push_block))
                    .with(Motion::new(4.0))
                    .with(Sprite { region: vec2(1, 3), layer: visual::OBJECT_LAYER })
                    .with(Collision::Pushable)
                    .build();
            }

            for button in &level.buttons
            {
                world.create_now()
                    .with(Position(*button))
                    .with(Sprite { region: vec2(2, 3), layer: visual::BG_LAYER })
                    .with(Button(false))
                    .build();
            }

            for gate in &level.gates
            {
                world.create_now()
                    .with(Position(*gate))
                    .with(Sprite { region: vec2(0, 4), layer: visual::BG_LAYER })
                    .with(Collision::Obstacle)
                    .with(ButtonGate(false))
                    .build();
            }
        }

        let planner = Planner::new(world);

        GameState
        {
            shader: shader,
            mesh: mesh,
            atlas: atlas,
            planner: planner,
            camera_pos: camera_pos,
            time: 0.0
        }
    }

    fn update(&mut self, dt: f64, game: &mut Game) -> bool
    {
        self.time += dt;
        let player_control_direction = game.input.dir();

        self.planner.run_custom(|arg| buttons::check_button_presses(arg));
        self.planner.run_custom(|arg| buttons::open_and_close_gates(arg));

        self.planner.run_custom(|arg| motion::track_player(arg));
        self.planner.run_custom(move |arg| motion::player_controls(arg, player_control_direction));
        self.planner.run_custom(|arg| motion::push_stuff(arg));
        self.planner.run_custom(move |arg| motion::move_towards_destinations(arg, dt));

        self.planner.run_custom(|arg| buttons::update_gate_sprites(arg));

        let exiting_state: bool;

        {
            let world = self.planner.mut_world();
            let victory = victory::determine_victory_from_goal(world);
            let gameover = victory::determine_gameover_from_hazard(world);
            if victory
            {
                game.current_level += 1;
                if game.current_level >= game.levels.len()
                {
                    game.complete = true;
                    game.current_level = 0;
                    game.current_state = StateType::EndingState;
                }
            }
            exiting_state = victory | gameover;
        }

        self.planner.wait();

        !exiting_state
    }

    fn draw(&mut self, target: &mut Frame, game: &mut Game)
    {
        target.clear_color_srgb_and_depth((0.75, 0.75, 0.75, 1.0), 1.0);

        let colormap = Sampler::new(&self.atlas.texture)
            .minify_filter(MinifySamplerFilter::Nearest)
            .magnify_filter(MagnifySamplerFilter::Nearest)
            .wrap_function(SamplerWrapFunction::Clamp);

        let projection = calculate_projection(game.resolution, game.tile_size);

        {
            let world = self.planner.mut_world();
            let (position, sprite) = (world.read::<Position>().pass(), world.read::<Sprite>().pass());

            let mut render_buffer = Vec::new();

            for (position, sprite) in (&position, &sprite).join()
            {
                let (uv_offset, uv_scale) = self.atlas.get_uv_offset_scale(sprite.region.components[0], sprite.region.components[1]);
                let pixel_position = (position.0 * game.tile_size as f32).round_i32();
                let rounded_position = vec2(pixel_position.components[0] as f32, pixel_position.components[1] as f32) * (1.0 / game.tile_size as f32);

                render_buffer.push((sprite.layer, rounded_position.components, uv_offset, uv_scale));
            }

            render_buffer.sort_by_key(|k| k.0);

            for (_layer, position, uv_offset, uv_scale) in render_buffer
            {
                target.draw(
                    &self.mesh.0,
                    &self.mesh.1,
                    &self.shader,
                    &uniform!
                    {
                        projection: projection,
                        camera_pos: self.camera_pos.components,
                        colormap: colormap,
                        position: position,
                        uv_offset: uv_offset,
                        uv_scale: uv_scale
                    },
                    &DrawParameters
                    {
                        depth: Depth
                        {
                            test: DepthTest::IfLess,
                            write: false,
                            .. Default::default()
                        },
                        blend: Blend::alpha_blending(),
                        .. Default::default()
                    }).unwrap();
            }
        }
    }
}
