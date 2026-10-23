//! dim3：3D 物理维度（feature `3d`）。
//!
//! 与 [`crate::dim2`] 完全镜像的接口面（同名类型、同语义），双开时经
//! 本模块路径使用；单维度时顶层 `rapier_starfish_debug::` 便利别名自动指向本维度。
//! 维度差异（深度通道契约/相机/轮廓拓扑）见设计文档 §7。

pub mod debug;
pub mod world;

pub use debug::render::DebugLinesRenderer;
pub use debug::{collect, DebugDraw, LineSink};
pub use world::PhysicsWorld;

/// dim3 便捷导入面：本维度全部类型 + rapier3d prelude。
pub mod prelude {
    pub use crate::dim3::debug::render::DebugLinesRenderer;
    pub use crate::dim3::debug::{collect, DebugDraw, LineSink};
    pub use crate::dim3::world::PhysicsWorld;
    pub use crate::step::StepAccumulator;
    pub use rapier3d::prelude::*;
}
