//! dim2：2D 物理维度（feature `2d`）。
//!
//! 与 [`crate::dim3`] 完全镜像的接口面（同名类型、同语义），双开时经
//! 本模块路径使用；单维度时顶层 `rapier_starfish_debug::` 便利别名自动指向本维度。

pub mod debug;
pub mod world;

pub use debug::render::DebugLinesRenderer;
pub use debug::{collect, DebugDraw, LineSink};
pub use world::PhysicsWorld;

/// dim2 便捷导入面：本维度全部类型 + rapier2d prelude。
///
/// 双开（2d + 3d）时本 prelude 与 dim3 prelude 各自独立——按模块路径
/// 分别导入即可（`use rapier_starfish_debug::dim2::prelude as p2;`）。
pub mod prelude {
    pub use crate::dim2::debug::render::DebugLinesRenderer;
    pub use crate::dim2::debug::{collect, DebugDraw, LineSink};
    pub use crate::dim2::world::PhysicsWorld;
    pub use crate::step::StepAccumulator;
    pub use rapier2d::prelude::*;
}
