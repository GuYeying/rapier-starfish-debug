//! dim2 debug 渲染数据面（设计文档 §5）：物理状态 → 线段流。
//!
//! 控制面 = rapier [`DebugRenderPipeline`] **直透**（`style`/`mode` 均为
//! pub 字段，配色/内容/类型零包装——设计文档 §0 "不懒人化"）：
//! - **渲染内容** = `DebugRenderMode` 位集
//!   （`COLLIDER_SHAPES` / `CONTACTS` / `JOINTS` / `COLLIDER_AABBS` /
//!   `RIGID_BODY_AXES` …）；
//! - **渲染类型** = `DebugRenderStyle`
//!   （按刚体类型配色 / 休眠乘数 / 接触法线长度 / 圆形细分度…）。
//!
//! ⚠️ [`DebugDraw`] 须**持久持有**（勿每帧重建）——pipeline 构造含细分
//! 轮廓缓存分配，逐帧重建 = 无谓分配。

pub mod render;
pub mod sink;

pub use render::DebugLinesRenderer;
pub use sink::LineSink;

use rapier2d::pipeline::{DebugRenderMode, DebugRenderPipeline, DebugRenderStyle};

use crate::dim2::world::PhysicsWorld;

/// debug 绘制状态（rapier pipeline 的持久持有壳；直透控制面）。
///
/// ```ignore
/// use rapier2d::pipeline::DebugRenderMode;
/// let mut draw = DebugDraw::new(DebugRenderMode::COLLIDER_SHAPES | DebugRenderMode::CONTACTS);
/// // 运行期直改控制面（pub 字段）：
/// draw.pipeline().mode.remove(DebugRenderMode::CONTACTS);
/// draw.pipeline().style.subdivisions = 16;
/// ```
pub struct DebugDraw {
    pipeline: DebugRenderPipeline,
}

impl DebugDraw {
    /// 按渲染内容位创建（style = rapier 默认）。
    pub fn new(mode: DebugRenderMode) -> Self {
        Self::with_style(mode, DebugRenderStyle::default())
    }

    /// 按渲染内容位 + 渲染类型创建。
    pub fn with_style(mode: DebugRenderMode, style: DebugRenderStyle) -> Self {
        Self {
            pipeline: DebugRenderPipeline::new(style, mode),
        }
    }

    /// 控制面直通（`style` / `mode` 均可运行期直改）。
    pub fn pipeline(&mut self) -> &mut DebugRenderPipeline {
        &mut self.pipeline
    }
}

impl Default for DebugDraw {
    fn default() -> Self {
        // 默认内容：碰撞体形状 + 接触点（物理调试主视野，设计文档 §8）
        Self::new(DebugRenderMode::COLLIDER_SHAPES | DebugRenderMode::CONTACTS)
    }
}

/// 物理状态 → 线段流（每帧调用；`sink` 内部 clear + 复用缓冲）。
pub fn collect(world: &PhysicsWorld, draw: &mut DebugDraw, sink: &mut LineSink) {
    sink.clear();
    world.get_ref().debug_render(&mut draw.pipeline, sink);
}

#[cfg(test)]
mod tests {
    use super::*;
    // 显式导入压过两侧 glob（本库与 rapier prelude 同名 PhysicsWorld）
    use crate::dim2::world::PhysicsWorld;
    use rapier2d::prelude::*;

    /// 地面（固定）+ 动球的最小场景
    fn sample_world() -> PhysicsWorld {
        let mut world = PhysicsWorld::new([0.0, -9.81]);
        world
            .get()
            .insert(RigidBodyBuilder::fixed(), ColliderBuilder::cuboid(5.0, 0.5));
        world.get().insert(
            RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 3.0)),
            ColliderBuilder::ball(0.5),
        );
        world
    }

    #[test]
    fn 空世界_空输出() {
        let world = PhysicsWorld::new([0.0, -9.81]);
        let mut draw = DebugDraw::default();
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert!(sink.is_empty());
    }

    #[test]
    fn 空mode位_有体也无输出() {
        let world = sample_world();
        let mut draw = DebugDraw::new(DebugRenderMode::empty());
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert!(sink.is_empty());
    }

    #[test]
    fn 形状模式_矩形四边对账() {
        // 仅固定地面（2D cuboid = 矩形轮廓 = 4 边）
        let mut world = PhysicsWorld::new([0.0, -9.81]);
        world
            .get()
            .insert(RigidBodyBuilder::fixed(), ColliderBuilder::cuboid(5.0, 0.5));
        let mut draw = DebugDraw::new(DebugRenderMode::COLLIDER_SHAPES);
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert_eq!(sink.line_count(), 4, "2D 矩形轮廓 = 4 线段");
        assert_eq!(sink.vertex_count(), 8);
    }

    #[test]
    fn 球细分度随style() {
        // 仅动球：subdivisions = n → 圆轮廓 n 段
        for sub in [8u32, 16] {
            let mut world = PhysicsWorld::new([0.0, -9.81]);
            world.get().insert(
                RigidBodyBuilder::dynamic(),
                ColliderBuilder::ball(0.5),
            );
            let style = DebugRenderStyle {
                subdivisions: sub,
                ..DebugRenderStyle::default()
            };
            let mut draw = DebugDraw::with_style(DebugRenderMode::COLLIDER_SHAPES, style);
            let mut sink = LineSink::new();
            collect(&world, &mut draw, &mut sink);
            // 实测：圆轮廓 = subdivisions + 1 段（n 点闭合补一段）
            assert_eq!(sink.line_count(), sub as usize + 1, "subdivisions = {sub}");
        }
    }

    #[test]
    fn mode位叠加_输出单调增() {
        let world = sample_world();
        let mut sink = LineSink::new();
        let mut count = |mode| {
            let mut draw = DebugDraw::new(mode);
            collect(&world, &mut draw, &mut sink);
            sink.line_count()
        };
        let shapes = count(DebugRenderMode::COLLIDER_SHAPES);
        let with_aabb = count(DebugRenderMode::COLLIDER_SHAPES | DebugRenderMode::COLLIDER_AABBS);
        assert!(shapes > 0);
        assert!(with_aabb > shapes, "叠加 AABB 位输出应增多");
    }

    #[test]
    fn 配色直透_style改色落顶点() {
        // 只有固定体：形状线颜色 = style.collider_fixed_color（HSLA 红 → RGBA 红）
        let mut world = PhysicsWorld::new([0.0, -9.81]);
        world
            .get()
            .insert(RigidBodyBuilder::fixed(), ColliderBuilder::cuboid(5.0, 0.5));
        let style = DebugRenderStyle {
            collider_fixed_color: [0.0, 1.0, 0.5, 1.0],
            ..DebugRenderStyle::default()
        };
        let mut draw = DebugDraw::with_style(DebugRenderMode::COLLIDER_SHAPES, style);
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert!(sink.vertex_count() > 0);
        for v in sink.geometry().vertices.iter() {
            assert!(
                (v.color[0] - 1.0).abs() < 1e-4 && v.color[1].abs() < 1e-4,
                "固定体线色应为 RGBA 红，实际 {:?}",
                v.color
            );
        }
    }

    #[test]
    fn 睡眠乘数直透_休眠体变暗() {
        // 静止不渲染≠没渲染：rapier 默认把休眠体线色亮度乘 0.2
        //（HSLA 乘数 [1,1,0.2,1]，深背景上近乎隐形）——控制面可改
        let mut world = PhysicsWorld::new([0.0, -9.81]);
        let (rb, _) = world.get().insert(
            RigidBodyBuilder::dynamic(),
            ColliderBuilder::ball(0.5),
        );
        world.get().bodies[rb].sleep(); // 强制休眠
        let style = DebugRenderStyle {
            sleep_color_multiplier: [1.0, 1.0, 0.2, 1.0], // rapier 默认
            ..DebugRenderStyle::default()
        };
        let mut draw = DebugDraw::with_style(DebugRenderMode::COLLIDER_SHAPES, style);
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        assert!(sink.vertex_count() > 0, "休眠体仍应产生线段（只是变暗）");
        for v in sink.geometry().vertices.iter() {
            let lum = 0.2126 * v.color[0] + 0.7152 * v.color[1] + 0.0722 * v.color[2];
            assert!(lum < 0.4, "休眠体线应为暗色（乘数生效），亮度 {lum}");
        }
        // 乘数改全 1 → 与清醒体同亮度
        draw.pipeline().style.sleep_color_multiplier = [1.0; 4];
        collect(&world, &mut draw, &mut sink);
        assert!(sink.vertex_count() > 0);
    }

    #[test]
    fn 复用收集_跨帧capacity稳定() {
        let mut world = sample_world();
        let mut draw = DebugDraw::default();
        let mut sink = LineSink::new();
        collect(&world, &mut draw, &mut sink);
        let cap = sink.geometry().vertices.capacity();
        let lines = sink.line_count();
        // 第二帧（含步进使球微动）收集：计数一致量级、capacity 不缩
        world.step();
        collect(&world, &mut draw, &mut sink);
        assert_eq!(sink.geometry().vertices.capacity(), cap);
        assert!(sink.line_count() >= lines - 1 && sink.line_count() <= lines + 4);
    }
}
