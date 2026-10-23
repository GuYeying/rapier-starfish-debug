//! debug_render_demo：M3 验收示例（设计文档 §8）。
//!
//! 场景：地面 + 三个下落体（盒/球/胶囊），物理调试线随刚体位姿。
//! 键位（渲染内容/类型控制面验收）：
//! - `1` 碰撞体形状 / `2` 接触点 / `3` 关节 / `4` AABB / `5` 刚体轴 —— 开关对应 mode 位
//! - `6` 睡眠体变暗标识开关（rapier 默认把休眠体线色调暗 80%——深背景上
//!   近乎隐形；关掉即休眠同色。静止不渲染≠没渲染，是状态编码）
//! - `+` / `-` 调米→像素缩放（相机缩放不影响物理——米制世界与视图解耦验收）
//! - `Esc` 退出
//!
//! 运行：`cargo run --example debug_render_demo`

use rapier_starfish_debug::dim2::prelude::*;
use rapier2d::pipeline::DebugRenderMode;
use starfish_render::render_entry::RenderEntry;
use starfish_we::next_frame;
use starfish_we::{Event, KeyCode, Window};
use wgpu::Color;
use std::time::Instant;

const CLEAR: Color = Color { r: 0.08, g: 0.08, b: 0.10, a: 1.0 };

async fn app_body() -> Result<(), Box<dyn std::error::Error>> {
    let window = Window::builder()
        .title("rapier-starfish-debug debug_render_demo（1-5 开关内容位，+/- 缩放，Esc 退出）")
        .size((960, 720))
        .build()?;
    let (_context, access, mut surface) =
        RenderEntry::async_new(&window, Default::default(), Default::default()).await?;

    // ── 物理场景：地面 + 阶梯 + 三种形状下落体 ──
    let mut world = PhysicsWorld::new([0.0, -9.81]);
    world.get().insert(
        RigidBodyBuilder::fixed().translation(Vector::new(0.0, -4.0)),
        ColliderBuilder::cuboid(9.0, 0.4),
    );
    world.get().insert(
        RigidBodyBuilder::fixed().translation(Vector::new(3.0, -2.5)).rotation(0.3),
        ColliderBuilder::cuboid(2.5, 0.15),
    );
    world.get().insert(
        RigidBodyBuilder::fixed().translation(Vector::new(-3.5, -1.2)).rotation(-0.25),
        ColliderBuilder::cuboid(2.0, 0.15),
    );
    world.get().insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(-1.5, 6.0)),
        ColliderBuilder::cuboid(0.5, 0.5),
    );
    world.get().insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 8.0)),
        ColliderBuilder::ball(0.5),
    );
    world.get().insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(1.5, 10.0)),
        ColliderBuilder::capsule_y(0.5, 0.3),
    );

    // ── debug 三件套（控制面/收集器/绘制器——全部持久持有）──
    let mut draw = DebugDraw::default();
    let mut sink = LineSink::new();
    let mut renderer = DebugLinesRenderer::new(&access, surface.sample_count());

    let mut ppm: f32 = 50.0;
    renderer.set_camera_ortho(&access, surface.size(), ppm);

    let mut last = Instant::now();
    let mut frames: u32 = 0;
    loop {
        // 事件先行（设计文档 §6 时序）
        while let Some(event) = window.poll_event() {
            match event {
                Event::CloseRequested => return Ok(()),
                Event::Resized { width, height } => {
                    surface.resize(width, height);
                    renderer.set_camera_ortho(&access, (width, height), ppm);
                }
                Event::KeyDown { key, .. } => match key {
                    KeyCode::Escape => return Ok(()),
                    KeyCode::Digit1 => draw.pipeline().mode.toggle(DebugRenderMode::COLLIDER_SHAPES),
                    KeyCode::Digit2 => draw.pipeline().mode.toggle(DebugRenderMode::CONTACTS),
                    KeyCode::Digit3 => draw.pipeline().mode.toggle(DebugRenderMode::JOINTS),
                    KeyCode::Digit4 => draw.pipeline().mode.toggle(DebugRenderMode::COLLIDER_AABBS),
                    KeyCode::Digit5 => draw.pipeline().mode.toggle(DebugRenderMode::RIGID_BODY_AXES),
                    KeyCode::Digit6 => {
                        // 睡眠/禁用配色乘数切换：rapier 默认（变暗标识状态）↔ 全 1（同色）
                        let s = &mut draw.pipeline().style;
                        let uniform = [1.0, 1.0, 1.0, 1.0];
                        let was_uniform = s.sleep_color_multiplier == uniform;
                        if was_uniform {
                            // 恢复 rapier 默认（debug 视图状态编码）
                            s.sleep_color_multiplier = [1.0, 1.0, 0.2, 1.0];
                            s.sleep_eligible_color_multiplier = [0.35, 1.0, 1.4, 1.0];
                            s.disabled_color_multiplier = [0.0, 0.0, 1.0, 1.0];
                        } else {
                            s.sleep_color_multiplier = uniform;
                            s.sleep_eligible_color_multiplier = uniform;
                            s.disabled_color_multiplier = uniform;
                        }
                        starfish_std::debug::console_log(&format!(
                            "[demo] 睡眠体变暗标识 = {}",
                            !was_uniform
                        ));
                    }
                    KeyCode::Equal | KeyCode::NumpadAdd => {
                        ppm = (ppm + 10.0).min(200.0);
                        renderer.set_camera_ortho(&access, surface.size(), ppm);
                        starfish_std::debug::console_log(&format!("[demo] 缩放 = {ppm} px/m"));
                    }
                    KeyCode::Minus | KeyCode::NumpadSubtract => {
                        ppm = (ppm - 10.0).max(10.0);
                        renderer.set_camera_ortho(&access, surface.size(), ppm);
                        starfish_std::debug::console_log(&format!("[demo] 缩放 = {ppm} px/m"));
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // 固定步长积分 → debug 收集（物理后、渲染前，设计文档 §6）
        let real_dt = last.elapsed().as_secs_f32().min(0.1);
        last = Instant::now();
        world.step_frame(real_dt);
        collect(&world, &mut draw, &mut sink);

        // 渲染：surface 清屏通道 + 自开绘制通道（与引擎时序合流）
        // ⚠️ 通道**不带深度附件**——gfx 2D 线管线深度关闭（管线目标格式
        // 必须 == 通道附件，wgpu 校验硬约束）
        if surface.begin_frame(CLEAR, 1.0) {
            let mut encoder = access.create_command_encoder();
            let color = surface.get_current_color_attachment();
            let color_atts = [&color]; // 局部绑定：通道借用须跨语句存活
            let mut pass =
                encoder.begin_render_pass("rapier_debug", &color_atts, None, None, None, None);
            renderer.draw(&access, &mut pass, &sink);
            pass.end();
            surface.submit_single(encoder.finish());
            surface.present();
        }

        frames += 1;
        if frames % 300 == 0 {
            starfish_std::debug::console_log(&format!(
                "[demo] 帧 {frames}，线段 {}，mode = {:?}",
                sink.line_count(),
                draw.pipeline().mode
            ));
        }
        next_frame().await;
    }
}

starfish_std::app_entry!(app_body());
