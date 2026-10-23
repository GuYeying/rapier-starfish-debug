//! debug_render_demo_3d：3D 维度验收示例（feature `3d`；设计文档 §7/§8）。
//!
//! 场景：3D 地面板 + 斜板 + 三个下落体（盒/球/胶囊），物理调试线随刚体
//! 位姿；环绕相机（orbit）观察。
//! 键位：
//! - `A` / `D` 相机环绕，`W` / `S` 拉近拉远（视图层——不影响物理）
//! - `1` 碰撞体形状 / `2` 接触点 / `3` 关节 / `4` AABB / `5` 刚体轴 —— 开关 mode 位
//! - `6` 睡眠体变暗标识开关（静止"消失"是 rapier 状态编码，非渲染缺失）
//! - `Esc` 退出
//!
//! 运行：`cargo run --features 3d --example debug_render_demo_3d`
//!
//! ⚠️ 与 2D 版相反：3D 线管线深度开启，**通道必须带深度附件**
//! （wgpu 目标格式严格相等校验，设计文档 §7）。

use rapier_starfish_debug::dim3::prelude::*;
use rapier3d::pipeline::DebugRenderMode;
use starfish_render::render_entry::RenderEntry;
use starfish_we::next_frame;
use starfish_we::{Event, KeyCode, Window};
use wgpu::Color;
use std::time::Instant;
//cargo run --no-default-features --features 3d --example debug_render_demo_3d
const CLEAR: Color = Color { r: 0.08, g: 0.08, b: 0.10, a: 1.0 };

async fn app_body() -> Result<(), Box<dyn std::error::Error>> {
    let window = Window::builder()
        .title("rapier-starfish-debug 3d demo（A/D 环绕 W/S 距离，1-5 内容位，6 睡眠标识，Esc 退出）")
        .size((960, 720))
        .build()?;
    let (_context, access, mut surface) =
        RenderEntry::async_new(&window, Default::default(), Default::default()).await?;

    // ── 物理场景：地面板 + 两块斜板 + 三种形状下落体 ──
    let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
    world.get().insert(
        RigidBodyBuilder::fixed().translation(Vector::new(0.0, -4.0, 0.0)),
        ColliderBuilder::cuboid(9.0, 0.4, 9.0),
    );
    world.get().insert(
        RigidBodyBuilder::fixed()
            .translation(Vector::new(3.0, -2.5, 0.0))
            .rotation(Vector::new(0.0, 0.0, 0.3)), // 欧拉角（绕 Z 倾斜）
        ColliderBuilder::cuboid(2.5, 0.15, 2.5),
    );
    world.get().insert(
        RigidBodyBuilder::fixed()
            .translation(Vector::new(-3.5, -1.2, 0.0))
            .rotation(Vector::new(0.0, 0.0, -0.25)),
        ColliderBuilder::cuboid(2.0, 0.15, 2.0),
    );
    world.get().insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(-1.5, 6.0, 0.0)),
        ColliderBuilder::cuboid(0.5, 0.5, 0.5),
    );
    world.get().insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 8.0, 0.0)),
        ColliderBuilder::ball(0.5),
    );
    world.get().insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(1.5, 10.0, 0.0)),
        ColliderBuilder::capsule_y(0.5, 0.3),
    );

    // ── debug 三件套（持久持有）──
    let mut draw = DebugDraw::default();
    let mut sink = LineSink::new();
    let mut renderer = DebugLinesRenderer::new(&access, surface.sample_count());

    // ── 环绕相机（视图状态；A/D 环绕、W/S 距离）──
    let mut yaw: f32 = 0.9; // 环绕角（rad）
    let mut dist: f32 = 10.0; // 相机距目标距离（米）
    let pitch: f32 = 0.45; // 俯仰（固定）
    let target = [0.0, -1.0, 0.0];
    let fov: f32 = 1.0; // 垂直视场角（rad）

    let update_camera = |renderer: &DebugLinesRenderer, access: &starfish_render::render_resource_access::RenderResourceAccess,
                         yaw: f32, dist: f32, viewport: (u32, u32)| {
        let eye = [
            target[0] + dist * yaw.cos() * pitch.cos(),
            target[1] + dist * pitch.sin(),
            target[2] + dist * yaw.sin() * pitch.cos(),
        ];
        renderer.set_camera_lookat(access, eye, target, fov, viewport);
    };
    update_camera(&renderer, &access, yaw, dist, surface.size());

    let mut last = Instant::now();
    let mut frames: u32 = 0;
    loop {
        // 事件先行（设计文档 §6 时序）
        while let Some(event) = window.poll_event() {
            match event {
                Event::CloseRequested => return Ok(()),
                Event::Resized { width, height } => {
                    surface.resize(width, height);
                    update_camera(&renderer, &access, yaw, dist, (width, height));
                }
                Event::KeyDown { key, .. } => match key {
                    KeyCode::Escape => return Ok(()),
                    KeyCode::KeyA => yaw += 0.15,
                    KeyCode::KeyD => yaw -= 0.15,
                    KeyCode::KeyW => dist = (dist - 2.0).max(4.0),
                    KeyCode::KeyS => dist = (dist + 2.0).min(80.0),
                    KeyCode::Digit1 => draw.pipeline().mode.toggle(DebugRenderMode::COLLIDER_SHAPES),
                    KeyCode::Digit2 => draw.pipeline().mode.toggle(DebugRenderMode::CONTACTS),
                    KeyCode::Digit3 => draw.pipeline().mode.toggle(DebugRenderMode::JOINTS),
                    KeyCode::Digit4 => draw.pipeline().mode.toggle(DebugRenderMode::COLLIDER_AABBS),
                    KeyCode::Digit5 => draw.pipeline().mode.toggle(DebugRenderMode::RIGID_BODY_AXES),
                    KeyCode::Digit6 => {
                        let s = &mut draw.pipeline().style;
                        let uniform = [1.0, 1.0, 1.0, 1.0];
                        let was_uniform = s.sleep_color_multiplier == uniform;
                        if was_uniform {
                            s.sleep_color_multiplier = [1.0, 1.0, 0.2, 1.0];
                            s.sleep_eligible_color_multiplier = [0.35, 1.0, 1.4, 1.0];
                            s.disabled_color_multiplier = [0.0, 0.0, 1.0, 1.0];
                        } else {
                            s.sleep_color_multiplier = uniform;
                            s.sleep_eligible_color_multiplier = uniform;
                            s.disabled_color_multiplier = uniform;
                        }
                        starfish_std::debug::console_log(&format!(
                            "[demo3d] 睡眠体变暗标识 = {}",
                            !was_uniform
                        ));
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // 固定步长积分 → debug 收集（物理后、渲染前）
        let real_dt = last.elapsed().as_secs_f32().min(0.1);
        last = Instant::now();
        world.step_frame(real_dt);
        collect(&world, &mut draw, &mut sink);

        // 渲染：⚠️ 3D 线管线深度开启——通道**必须带深度附件**
        if surface.begin_frame(CLEAR, 1.0) {
            let mut encoder = access.create_command_encoder();
            let color = surface.get_current_color_attachment();
            let color_atts = [&color];
            let depth = surface.get_current_depth_attachment();
            let mut pass =
                encoder.begin_render_pass("rapier_debug_3d", &color_atts, depth, None, None, None);
            renderer.draw(&access, &mut pass, &sink);
            pass.end();
            surface.submit_single(encoder.finish());
            surface.present();
        }

        frames += 1;
        if frames % 300 == 0 {
            starfish_std::debug::console_log(&format!(
                "[demo3d] 帧 {frames}，线段 {}，mode = {:?}",
                sink.line_count(),
                draw.pipeline().mode
            ));
        }
        next_frame().await;
    }
}

starfish_std::app_entry!(app_body());
