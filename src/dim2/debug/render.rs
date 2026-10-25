//! dim2 debug 绘制面：gfx 积木组装（设计文档 §2 定稿——v1 不自写 wgsl）。
//!
//! 组成：`shape.wgsl` 线管线（`line_pipeline_2d`）+ 相机 uniform +
//! 自持动态 VBO/IBO。
//!
//! ⚠️ 刻意**不走** gfx `Mesh` 持缓冲：`Mesh.index_count` 是创建期定值
//! （`write_index_buffer` 只传字节不更新计数），不适配每帧变化的索引数
//! ——这里索引数由 [`LineSink`] 实时决定，绘制走 `set_vertex_buffer` /
//! `set_index_buffer` / `draw_indexed` 公共底层（索引数精确、零重建）。
//! 管线构建仍借一次 `shape_mesh`（只为提取 `ShapeVertex` 顶点布局）。

use std::sync::Arc;

use starfish_gfx::geometry::Geometry;
use starfish_gfx::pipeline::{line_pipeline_2d, shape_mesh};
use starfish_gfx::ShapeVertex;
use starfish_render::bind_group::bind_group::BindGroup;
use starfish_render::mesh::mesh::Mesh;
use starfish_render::pipeline::RenderPipeline;
use starfish_render::render_pass::render_pass::RenderPass;
use starfish_render::render_resource_access::RenderResourceAccess;
use wgpu::BufferUsages;

use super::sink::LineSink;

/// 物理调试线渲染器（2D；gfx 积木组装）。
///
/// - 相机：正交、**世界原点 = 视口中心**、米→像素缩放 =
///   [`Self::set_camera_ortho`] 的 `pixels_per_meter`；
/// - 管线格式随 [`RenderResourceAccess`] 的默认表面格式（格式随
///   `RenderSurface`，构建于表面就位后）；
/// - ⚠️ **须在无深度附件的通道中绘制**——gfx 2D 线管线深度关闭，
///   wgpu 校验要求管线深度目标与通道附件严格一致（`None` vs `Some`）。
pub struct DebugLinesRenderer {
    /// 顶点布局供体（管线构建期提取 `ShapeVertex` 布局；保留便于重建）
    #[allow(dead_code)]
    layout_mesh: Mesh,
    pipeline: Arc<RenderPipeline>,
    camera_buf: Arc<wgpu::Buffer>,
    camera_bind: BindGroup,
    /// 动态顶点/索引缓冲（自持；只增扩容）
    vbo: Arc<wgpu::Buffer>,
    ibo: Arc<wgpu::Buffer>,
    /// 当前缓冲容量（顶点数 / 索引数）
    capacity: (usize, usize),
}

impl DebugLinesRenderer {
    /// 装配（须在表面就位后调用——管线格式取 access 的默认表面格式）。
    pub fn new(access: &RenderResourceAccess, sample_count: u32) -> Self {
        // 布局供体：ShapeVertex 2 顶点 + 2 索引的占位几何
        let mut donor_geo = Geometry::default();
        donor_geo.push([0.0; 3], [0.0; 4]);
        donor_geo.push([0.0; 3], [0.0; 4]);
        donor_geo.indices.push(0);
        donor_geo.indices.push(1);
        let layout_mesh = shape_mesh(access, &donor_geo);

        let camera_buf = access.create_raw_buffer(
            Some("rapier_debug_camera"),
            64, // mat4x4<f32>
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );
        let camera_bind = access
            .bind_group_builder()
            .uniform_raw(0, camera_buf.clone(), 64)
            .build(Some("rapier_debug_camera_bind"));

        let pipeline = line_pipeline_2d(access, &camera_bind, &layout_mesh, sample_count.max(1));

        // 动态缓冲：初始 256 顶点 / 512 索引（不足时扩容重建）
        let vert_bytes = (256 * ShapeVertex::size()) as u64;
        let idx_bytes = (512 * std::mem::size_of::<u16>()) as u64;
        let vbo = access.create_raw_buffer(
            Some("rapier_debug_vbo"),
            vert_bytes,
            BufferUsages::VERTEX | BufferUsages::COPY_DST,
        );
        let ibo = access.create_raw_buffer(
            Some("rapier_debug_ibo"),
            idx_bytes,
            BufferUsages::INDEX | BufferUsages::COPY_DST,
        );

        Self {
            layout_mesh,
            pipeline,
            camera_buf,
            camera_bind,
            vbo,
            ibo,
            capacity: (256, 512),
        }
    }

    /// 正交相机（**2D**）：世界原点对视口中心，`pixels_per_meter` =
    /// 米→像素缩放。每次视口/缩放变化后调用（本帧生效）。
    pub fn set_camera_ortho(
        &self,
        access: &RenderResourceAccess,
        viewport_px: (u32, u32),
        pixels_per_meter: f32,
    ) {
        let m = ortho_mvp(viewport_px, pixels_per_meter);
        access.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&m));
    }

    /// 任意 MVP 直写（**全局相机对齐原语**，渲染架构批次 B）：宿主把
    /// 全局相机矩阵（starfish-pygame `camera_mvp_for` 产出，列主
    /// `[[f32; 4]; 4]`）写入——调试线与引擎管线同一定约（group0/b0 =
    /// 64B mat4 MVP），set_camera 一处设置、全体生效。
    pub fn set_camera_mvp(&self, access: &RenderResourceAccess, mvp: [[f32; 4]; 4]) {
        access.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&mvp));
    }

    /// 相机 bind group（供宿主**自建管线共享同一相机**——shape.wgsl 契约：
    /// group0/binding0 = 64B mat4 MVP。宿主管线的布局含同一绑定即可在
    /// 同一相机下绘制自己的几何）。
    pub fn camera_bind(&self) -> &BindGroup {
        &self.camera_bind
    }

    /// 编码绘制（调用方已 `begin` 的通道；可与其它绘制内容自由合批）。
    ///
    /// 空 sink 直接返回（无命令产生）。缓冲不足时按 2 倍扩容重建
    /// （只增不减——线数逐帧抖动不触发反复重建）。
    pub fn draw(
        &mut self,
        access: &RenderResourceAccess,
        pass: &mut RenderPass,
        sink: &LineSink,
    ) {
        let (vc, ic) = (sink.vertex_count(), sink.geometry().indices.len());
        if ic == 0 {
            return;
        }
        if vc > self.capacity.0 || ic > self.capacity.1 {
            // 顶点容量钳在 u16 上限（sink 守卫同款；超出部分已被 sink 丢弃）
            let nv = (vc.max(self.capacity.0) * 2).min(u16::MAX as usize);
            let ni = ic.max(self.capacity.1) * 2;
            self.vbo = access.create_raw_buffer(
                Some("rapier_debug_vbo"),
                (nv * ShapeVertex::size()) as u64,
                BufferUsages::VERTEX | BufferUsages::COPY_DST,
            );
            self.ibo = access.create_raw_buffer(
                Some("rapier_debug_ibo"),
                (ni * std::mem::size_of::<u16>()) as u64,
                BufferUsages::INDEX | BufferUsages::COPY_DST,
            );
            self.capacity = (nv, ni);
        }
        access.write_buffer(&self.vbo, 0, sink.geometry().vertices_bytes());
        access.write_buffer(&self.ibo, 0, sink.geometry().index_bytes());

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind);
        pass.set_vertex_buffer(0, &self.vbo);
        pass.set_index_buffer(&self.ibo, wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..ic as u32, 0, 0..1);
    }
}

/// 正交 MVP（列主序，wgpu NDC；z 行直通——2D 线管线深度关闭）。
///
/// 世界原点 = 视口中心；x/y 缩放 = `2 × pixels_per_meter / 视口边长`。
/// **公开**——宿主自建管线（CPU 侧.billboard 等）可复用同一相机数学。
#[rustfmt::skip]
pub fn ortho_mvp(viewport_px: (u32, u32), pixels_per_meter: f32) -> [[f32; 4]; 4] {
    let (w, h) = (viewport_px.0.max(1) as f32, viewport_px.1.max(1) as f32);
    let sx = 2.0 * pixels_per_meter / w;
    let sy = 2.0 * pixels_per_meter / h;
    [
        [sx,  0.0, 0.0, 0.0],
        [0.0, sy,  0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 正交矩阵_缩放值与单位结构() {
        let m = ortho_mvp((800, 600), 50.0);
        // x: 2×50/800 = 0.125；y: 2×50/600 ≈ 0.1667
        assert!((m[0][0] - 0.125).abs() < 1e-6);
        assert!((m[1][1] - 2.0 * 50.0 / 600.0).abs() < 1e-6);
        // 平移列 = 0（中心原点），对角线其余 = 1
        assert!((m[3][3] - 1.0).abs() < 1e-9);
        assert!((m[3][0]).abs() < 1e-9 && (m[3][1]).abs() < 1e-9);
        // 零尺寸视口钳 1
        let m1 = ortho_mvp((0, 0), 1.0);
        assert!((m1[0][0] - 2.0).abs() < 1e-9);
    }
}
