//! rapier-starfish-debug：Rapier 物理适配层（starfish 生态；2D/3D 双维度）。
//!
//! 设计文档 = 本仓库 `doc/设计文档.md`（2026-10-23 v0.5）。
//!
//! 职责与原则：
//! - **薄封装**（[`dim2::world::PhysicsWorld`] / [`dim3::world::PhysicsWorld`]）：
//!   生命周期 + 固定步长积分 + 逃生口——不复刻 rapier API，构建/查询/
//!   关节/事件全走 `get()` 直通 rapier 原生对象；
//! - **通用 debug 渲染**：物理状态 → 线段流收集器 + 绘制器，渲染内容/
//!   类型经 `DebugRenderMode`/`DebugRenderStyle` 直透控制（§2/§5）；
//! - 依赖方向：`rapier-starfish-debug → {rapier2d|rapier3d, starfish-render,
//!   starfish-gfx, starfish-std}`——**零 starfish-we 依赖**（开窗/循环
//!   归宿主，we 仅示例 dev-deps，设计文档 §3）。
//!
//! # 维度（feature `2d` / `3d`，**可双开**）
//!
//! - 单维度：顶层 `rapier_starfish_debug::prelude::*` 自动指向该维度（便利别名）；
//! - **双开**：经模块路径分别使用——`rapier_starfish_debug::dim2::*` 与
//!   `rapier_starfish_debug::dim3::*`（同名类型并存，各带自己的 rapier prelude）：
//!
//! ```ignore
//! use rapier_starfish_debug::dim2::prelude as p2;
//! use rapier_starfish_debug::dim3::prelude as p3;
//!
//! let mut w2 = p2::PhysicsWorld::new([0.0, -9.81]);        // 2D 玩法物理
//! let mut w3 = p3::PhysicsWorld::new([0.0, -9.81, 0.0]);   // 3D 表现物理
//! ```
//!
//! # 标准用法（单维度便利形态）
//!
//! ```ignore
//! use rapier_starfish_debug::prelude::*; // = 当前维度（默认 2d）
//!
//! let mut world = PhysicsWorld::new([0.0, -9.81]);
//! world.get().insert(
//!     RigidBodyBuilder::fixed().translation(Vector::new(0.0, -3.0)),
//!     ColliderBuilder::cuboid(5.0, 0.2),
//! );
//! loop {
//!     world.step_frame(real_dt);                         // 固定步长积分
//!     let y = world.get().bodies[ball].translation().y;  // 读位姿
//!     // … debug 渲染 / 呈现 …
//! }
//! ```

#[cfg(feature = "2d")]
pub mod dim2;
#[cfg(feature = "3d")]
pub mod dim3;
pub mod hsla;
pub mod step;

/// 顶层便捷导入面：单维度开启时 = 该维度全量（含其 rapier prelude）；
/// **双开时不自动展开**（同名冲突）——按 [`dim2`] / [`dim3`] 路径导入。
pub mod prelude {
    pub use crate::step::StepAccumulator;
    #[cfg(all(feature = "2d", not(feature = "3d")))]
    pub use crate::dim2::prelude::*;
    #[cfg(all(feature = "3d", not(feature = "2d")))]
    pub use crate::dim3::prelude::*;
}

// ── 单维度便利别名（顶层 = 该维度；双开时经 dim2/dim3 路径使用）──
#[cfg(all(feature = "2d", not(feature = "3d")))]
pub use crate::dim2::{collect, DebugDraw, DebugLinesRenderer, LineSink, PhysicsWorld};
#[cfg(all(feature = "3d", not(feature = "2d")))]
pub use crate::dim3::{collect, DebugDraw, DebugLinesRenderer, LineSink, PhysicsWorld};
