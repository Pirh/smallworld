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
    time: f64
}

impl State for GameState
{
    fn new(display: &Display) -> Self
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

        world.create_now()
            .with(Position(vec2(0.0, 0.0)))
            .with(Sprite { region: vec2(0, 2) })
            .with(Collision::Obstacle)
            .build();

        world.create_now()
            .with(Position(vec2(0.0, 4.0)))
            .with(Sprite { region: vec2(0, 3) })
            .with(Goal)
            .build();

        world.create_now()
            .with(Position(vec2(2.0, 0.0)))
            .with(Sprite { region: vec2(0, 0) })
            .with(Motion { destination: None, speed: 4.0 })
            .with(Player)
            .build();

        world.create_now()
            .with(Position(vec2(4.0, 0.0)))
            .with(Sprite { region: vec2(0, 1) })
            .with(Motion { destination: Some(motion::Destination { position: vec2(4.0, 4.0), direction: vec2(0.0, 1.0) }), speed: 4.0 })
            .with(Hazard)
            .build();

        let planner = Planner::new(world);

        GameState
        {
            shader: shader,
            mesh: mesh,
            atlas: atlas,
            planner: planner,
            time: 0.0
        }
    }

    fn update(&mut self, dt: f64, game: &mut Game) -> bool
    {
        self.time += dt;
        let player_control_direction = game.input.dir();

        self.planner.run_custom(move |arg| motion::player_controls(arg, player_control_direction));
        self.planner.run_custom(move |arg| motion::move_towards_destinations(arg, dt));

        let exiting_state: bool;
        {
            let world = self.planner.mut_world();
            let victory = victory::determine_victory_from_goal(world);
            let gameover = victory::determine_gameover_from_hazard(world);
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
            for (position, sprite) in (&position, &sprite).join()
            {
                let (uv_offset, uv_scale) = self.atlas.get_uv_offset_scale(sprite.region.components[0], sprite.region.components[1]);
                let pixel_position = (position.0 * game.tile_size as f32).round_i32();
                let rounded_position = vec2(pixel_position.components[0] as f32, pixel_position.components[1] as f32) * (1.0 / game.tile_size as f32);

                target.draw(
                    &self.mesh.0,
                    &self.mesh.1,
                    &self.shader,
                    &uniform!
                    {
                        projection: projection,
                        colormap: colormap,
                        position: rounded_position.components,
                        uv_offset: uv_offset,
                        uv_scale: uv_scale
                    },
                    &DrawParameters
                    {
                        depth: Depth
                        {
                            test: DepthTest::IfLess,
                            write: true,
                            .. Default::default()
                        },
                        blend: Blend::alpha_blending(),
                        .. Default::default()
                    }).unwrap();
            }
        }
    }
}
