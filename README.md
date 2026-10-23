# rapier-starfish-debug

Rapier 0.36 物理适配层（starfish 生态）：物理世界薄封装 + **通用级 debug
渲染**（接口控制渲染内容与类型，免定制）。

- Rust 1.85 / edition 2024；rapier 0.36（debug-render + parallel）/ wgpu 30
- 2D / 3D 双维度（feature `2d` / `3d`，**可同时开启**，按 `dim2::` / `dim3::`
  命名空间并存）
- 设计文档（架构/接口/坑位全集）：[`doc/设计文档.md`](doc/设计文档.md)

## 设计原则（"不懒人化"）

- **不复刻 rapier API**：`PhysicsWorld` 薄封装（生命周期 + 固定步长积分 +
  仿真时钟），构建/查询/关节/事件经 `world.get()` 直通 rapier 原生对象；
- **控制面直透**：渲染内容 = `DebugRenderMode` 位集、渲染类型 =
  `DebugRenderStyle`（原生配置零包装，运行期可改）；
- **零 starfish-we 依赖**：开窗/事件/主循环归宿主（poll 模型），we 仅示例
  dev-deps；
- **非注入式渲染**：`DebugLinesRenderer` 自持管线（gfx 积木装配），借调用方
  `RenderPass` 绘制——与宿主自建管线正交并存，`camera_bind()` /
  `ortho_mvp` / `lookat_pvp` 公开可复用（同一相机契约）。

## 快速上手

```toml
[dependencies]
rapier-starfish-debug = { path = "../rapier-starfish-debug", features = ["2d"] }
```

```rust
use rapier_starfish_debug::prelude::*; // = 当前维度（或 dim2/dim3::prelude 显式导入）

let mut world = PhysicsWorld::new([0.0, -9.81]);      // 2D；3D = [0.0, -9.81, 0.0]
world.get().insert(                                   // rapier builder 直通
    RigidBodyBuilder::fixed(),
    ColliderBuilder::cuboid(5.0, 0.5),
);

loop {
    world.step_frame(real_dt);                        // 固定步长积分（上限节流）
    collect(&world, &mut draw, &mut sink);            // 物理状态 → 线段流
    if surface.begin_frame(CLEAR, 1.0) { /* 自开 RenderPass → renderer.draw(...) */ }
}
```

## 验收示例

```bash
cargo run  --features 2d                  --example debug_render_demo_2d
cargo run  --no-default-features --features 3d --example debug_render_demo_3d
cargo test --features 2d,3d               # 双开全量（26/27/44 单测）
```

键位（两示例一致）：`1~5` 开关渲染内容位（形状/接触/关节/AABB/刚体轴）、
`6` 睡眠体变暗标识（静止"消失"是 rapier 状态编码，非渲染缺失）、
2D `+/-` 缩放；3D `A/D` 环绕、`W/S` 距离。

## 依赖须知

当前依赖形态为**同机路径依赖**（设计文档 §3）：本仓库与
[starfish](https://github.com/GuYeying/starfish) 主线共存于同级目录
（`../starfish`），依赖 `starfish-render` / `starfish-gfx` / `starfish-std`
（示例另需 `starfish-we`）。克隆本仓库后如路径不符，clone starfish 到同级
目录即可；后续可换 git/publish 依赖（wgpu 版本契约：须与 starfish-render
解析到同版本 30.0.1）。
