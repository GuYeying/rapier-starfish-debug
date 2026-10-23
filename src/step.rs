//! 固定步长积分器（accumulator）——纯逻辑组件，无 rapier 依赖。
//!
//! 把不均匀的真实帧时长折算成整物理步数：真实 dt 存入余量，
//! 攒够一个固定步长（[`Self::dt`]）就走一步，残差滚动到下帧。
//! 上限节流（对齐 starfish `time.Clock` 节流哲学）：余量封顶
//! `dt × max_steps`——帧时间骤增（卡顿/断点）不引发"死亡螺旋"
//! （帧越长 → 追帧步数越多 → 更长的恶性循环），超出部分直接丢弃。
//!
//! 语义对齐设计文档 §6 帧循环时序定约。

/// 固定步长积分器。
///
/// 单位约定：dt 与 real_dt 均为**秒**（对齐 starfish `time.Clock::tick`
/// 返秒语义）。
#[derive(Debug, Clone)]
pub struct StepAccumulator {
    dt: f32,
    max_steps: u32,
    accumulated: f32,
}

impl StepAccumulator {
    /// 按固定步长创建（`max_steps` 默认 5 = 单帧最多追 5 步）。
    pub fn new(dt: f32) -> Self {
        Self::with_limit(dt, 5)
    }

    /// 按固定步长 + 单帧追帧上限创建。
    ///
    /// `dt` 会被钳制到 > 0（防除零）；`max_steps` 钳制到 ≥ 1。
    pub fn with_limit(dt: f32, max_steps: u32) -> Self {
        Self {
            dt: if dt > 1e-6 { dt } else { 1e-6 },
            max_steps: max_steps.max(1),
            accumulated: 0.0,
        }
    }

    /// 固定步长（秒）。
    pub fn dt(&self) -> f32 {
        self.dt
    }

    /// 修改固定步长（秒）；已有余量保留（换 dt 后按新步长折算）。
    pub fn set_dt(&mut self, dt: f32) {
        self.dt = if dt > 1e-6 { dt } else { 1e-6 };
    }

    /// 单帧追帧步数上限。
    pub fn max_steps(&self) -> u32 {
        self.max_steps
    }

    /// 当前滚动余量（秒；不足一个 dt）。
    pub fn accumulated(&self) -> f32 {
        self.accumulated
    }

    /// 存入真实帧时长（负值按 0 处理），返回应执行的整步数。
    ///
    /// 余量先封顶（`dt × max_steps`）再折算——封顶即节流，超出上限的
    /// 真实时间被丢弃而非累积追帧。
    pub fn advance(&mut self, real_dt: f32) -> u32 {
        self.accumulated += real_dt.max(0.0);
        let cap = self.dt * self.max_steps as f32;
        if self.accumulated > cap {
            self.accumulated = cap;
        }
        let mut steps = 0u32;
        while self.accumulated >= self.dt {
            self.accumulated -= self.dt;
            steps += 1;
        }
        // 整除尽时连减可能留 -ε 级浮点残渣——钳零（负余量无意义）
        if self.accumulated < 0.0 {
            self.accumulated = 0.0;
        }
        steps
    }

    /// 清空余量（场景重置/时间跳变时用——跳变的时间不该被追帧消化）。
    pub fn reset(&mut self) {
        self.accumulated = 0.0;
    }

    /// 渲染插值系数（0..1）：余量占一个固定步长的比例。
    ///
    /// M4 渲染插值预留位（设计文档 §6——v1 不消费）：渲染态 = 当前物理态
    /// 与下一物理态按 alpha 混合可消除步进残差的微小抖动。
    pub fn alpha(&self) -> f32 {
        self.accumulated / self.dt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn 整步折算_整除帧() {
        let mut acc = StepAccumulator::new(DT);
        assert_eq!(acc.advance(DT), 1);
        assert_eq!(acc.advance(DT * 2.0), 2);
        assert_eq!(acc.advance(1.0 / 30.0), 2);
    }

    #[test]
    fn 残差滚动_不足一步不出步() {
        let mut acc = StepAccumulator::new(DT);
        // 半步 × 3 = 1 步 + 半步余量（(1/120)×3 = 1.25 个 DT）
        let half = DT / 2.0;
        assert_eq!(acc.advance(half), 0);
        assert_eq!(acc.advance(half), 1);
        assert_eq!(acc.advance(half), 0);
        // 余量 = 0.5×DT
        assert!((acc.alpha() - 0.5).abs() < 1e-5);
    }

    #[test]
    fn 微小帧时长_不出步且不丢失() {
        let mut acc = StepAccumulator::new(DT);
        let mut total = 0;
        for _ in 0..100 {
            total += acc.advance(DT / 10.0); // 每帧 1/10 步
        }
        // 100 帧攒出 10 步（余量 < 1 步）
        assert_eq!(total, 10);
    }

    #[test]
    fn 死亡螺旋封顶_超出上限的时间被丢弃() {
        let mut acc = StepAccumulator::new(DT); // max_steps = 5
        // 一次灌入 10 秒（≈600 步）→ 封顶只走 5 步，余量归零
        assert_eq!(acc.advance(10.0), 5);
        // 整除尽时连减留 ε 级浮点残渣（f32 误差，非语义余量）
        assert!(acc.accumulated().abs() < 1e-6, "残渣 = {}", acc.accumulated());
        assert_eq!(acc.advance(DT), 1); // 封顶后恢复正常节奏
    }

    #[test]
    fn 负帧时长按零处理() {
        let mut acc = StepAccumulator::new(DT);
        assert_eq!(acc.advance(-1.0), 0);
        assert_eq!(acc.accumulated(), 0.0);
    }

    #[test]
    fn 非法dt被钳制_换dt保留余量() {
        let mut acc = StepAccumulator::new(0.0);
        assert_eq!(acc.dt(), 1e-6); // 钳到正数防除零
        acc.set_dt(DT);
        assert_eq!(acc.advance(DT / 2.0), 0);
        acc.set_dt(DT * 2.0); // 换步长：余量半步仍在（不足新步长）
        assert_eq!(acc.advance(DT * 3.0 / 2.0), 1);
    }

    #[test]
    fn reset_清空余量() {
        let mut acc = StepAccumulator::new(DT);
        acc.advance(DT / 2.0);
        acc.reset();
        assert_eq!(acc.accumulated(), 0.0);
        assert_eq!(acc.advance(DT / 2.0), 0);
    }
}
