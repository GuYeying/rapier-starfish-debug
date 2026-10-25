//! dim3 物理世界封装：rapier3d 0.36 `PhysicsWorld` 的薄壳。
//!
//! "不懒人化"定约（设计文档 §0/§5）：**不复刻 rapier API**——构建面
//! 经 [`PhysicsWorld::get`] 直通 rapier 原生对象；封装层只做 starfish
//! 集成需要的增量：固定步长积分器（[`Self::step_frame`]，设计文档 §6）
//! 与仿真时钟。接口面与 dim2 完全镜像（维度互斥 feature 下同名双编译，
//! 双开时经 `rapier_starfish_debug::dim3::` 路径使用）。

use rapier3d::pipeline::PhysicsWorld as RapierPhysicsWorld;
// rapier3d 0.36 的 `Vector` 为零泛参具体别名（Vec3 形态，不可写 Vector<f32>）
use rapier3d::prelude::Vector;

use crate::step::StepAccumulator;

/// 物理世界（rapier [`RapierPhysicsWorld`] 薄封装；3D）。
///
/// 单位约定与步进语义见 dim2 同名文档；重力如 `[0.0, -9.81, 0.0]`。
pub struct PhysicsWorld {
    inner: RapierPhysicsWorld,
    clock: StepAccumulator,
    sim_time: f32,
}

impl PhysicsWorld {
    /// 创建物理世界（`gravity` = 重力加速度，如 `[0.0, -9.81, 0.0]`）。
    pub fn new(gravity: impl Into<Vector>) -> Self {
        let mut inner = RapierPhysicsWorld::new();
        inner.gravity = gravity.into();
        Self {
            inner,
            clock: StepAccumulator::new(1.0 / 60.0),
            sim_time: 0.0,
        }
    }

    /// 逃生口：完整 rapier 面（insert/查询/关节/事件/事件队列…全走这里）。
    pub fn get(&mut self) -> &mut RapierPhysicsWorld {
        &mut self.inner
    }

    /// 只读逃生口（仅需 `&` 的场景：debug 收集/射线查询/位姿读取）。
    pub fn get_ref(&self) -> &RapierPhysicsWorld {
        &self.inner
    }

    /// 固定步长步进一次（dt 见 [`Self::dt`]）。
    pub fn step(&mut self) {
        self.inner.step();
        self.sim_time += self.dt();
    }

    /// 实际帧时长 → 固定步长积分（帧循环主入口；上限节流防死亡螺旋）。
    pub fn step_frame(&mut self, real_dt: f32) -> u32 {
        let steps = self.clock.advance(real_dt);
        for _ in 0..steps {
            self.inner.step();
        }
        self.sim_time += steps as f32 * self.dt();
        steps
    }

    /// 带事件处理的 [`Self::step_frame`]（碰撞/触发力事件经 handler 回调
    /// ——碰撞体须 `active_events(COLLISION_EVENTS)` 选中。宿主自持
    /// `ChannelEventCollector` 传入即可，step 次数语义不变）。
    pub fn step_frame_with_events(
        &mut self,
        real_dt: f32,
        handler: &dyn rapier3d::prelude::EventHandler,
    ) -> u32 {
        let steps = self.clock.advance(real_dt);
        for _ in 0..steps {
            self.inner.step_with_events(&(), handler);
        }
        self.sim_time += steps as f32 * self.dt();
        steps
    }

    /// 固定步长（秒）。
    pub fn dt(&self) -> f32 {
        self.inner.integration_parameters.dt
    }

    /// 修改固定步长（秒；同步积分器余量语义不变）。
    pub fn set_dt(&mut self, dt: f32) {
        self.inner.integration_parameters.dt = dt;
        self.clock.set_dt(dt);
    }

    /// 已仿真时长（秒；= 累计 step 次数 × dt，与真实帧时间无关）。
    pub fn sim_time(&self) -> f32 {
        self.sim_time
    }

    /// 渲染插值系数（0..1，M4 预留——v1 不消费）。
    pub fn alpha(&self) -> f32 {
        self.clock.alpha()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim3::world::PhysicsWorld;
    use rapier3d::prelude::*;

    /// 自由落体：h(t) = h₀ - ½·g·t²（半隐式欧拉积分，容差放宽）
    #[test]
    fn 自由落体_位移对账() {
        let g = 9.81;
        let mut world = PhysicsWorld::new([0.0, -g, 0.0]);
        let (ball, _) = world.get().insert(
            RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 10.0, 0.0)),
            ColliderBuilder::ball(0.5),
        );

        let dt = world.dt();
        let steps = 60; // 1 秒
        for _ in 0..steps {
            world.step();
        }

        let y = world.get().bodies[ball].translation().y;
        let expected = 10.0 - 0.5 * g * (steps as f32 * dt).powi(2);
        assert!(
            (y - expected).abs() < 0.1,
            "自由落体 y = {y}，期望 ≈ {expected}"
        );
    }

    /// 落地静止冒烟：动球掉在固定地面（顶面 y = 0.5）上，5 秒后
    /// 停在接触高度附近且进入休眠
    #[test]
    fn 落地静止_冒烟() {
        let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
        world.get().insert(
            RigidBodyBuilder::fixed(),
            ColliderBuilder::cuboid(5.0, 0.5, 5.0),
        );
        let (ball, _) = world.get().insert(
            RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 5.0, 0.0)),
            ColliderBuilder::ball(0.5),
        );

        for _ in 0..300 {
            world.step();
        }
        let body = &world.get().bodies[ball];
        let y = body.translation().y;
        assert!((y - 1.0).abs() < 0.05, "落球 y = {y}，期望 ≈ 1.0");
        assert!(body.is_sleeping(), "5 秒后应已休眠（完全静止）");
    }

    #[test]
    fn step_frame_整除折算与残差滚动() {
        let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
        let dt = world.dt();
        assert_eq!(world.step_frame(dt), 1);
        assert_eq!(world.step_frame(dt * 2.0), 2);
        assert_eq!(world.step_frame(dt / 2.0), 0);
        assert_eq!(world.step_frame(dt / 2.0), 1);
        assert_eq!(world.step_frame(-1.0), 0);
    }

    #[test]
    fn step_frame_追帧封顶与仿真时钟() {
        let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
        assert_eq!(world.step_frame(10.0), 5);
        assert!((world.sim_time() - 5.0 / 60.0).abs() < 1e-6);
    }

    #[test]
    fn set_dt_同步积分器() {
        let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
        world.set_dt(1.0 / 30.0);
        assert_eq!(world.dt(), 1.0 / 30.0);
        assert_eq!(world.step_frame(1.0 / 30.0), 1);
        assert!((world.sim_time() - 1.0 / 30.0).abs() < 1e-6);
    }
}
