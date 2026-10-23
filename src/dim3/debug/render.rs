//! dim3 debug 绘制面：gfx 积木组装（与 dim2 镜像；差异见下）。
//!
//! - 管线 = `line_pipeline_3d`（**深度开启**，Depth24Plus）；
//! - 相机 = `set_camera_lookat`（手写 look-at + 透视，RH / wgpu 深度
//!   0..1，不触 glam deprecated 投影 API）；
//! - ⚠️ 通道契约与 2D **相反**：必须**带**深度附件（wgpu 目标格式严格
//!   相等校验）；
//! - 动态 VBO/IBO 自持（`Mesh.index_count` 创建期定值坑，见 dim2 文档）。

use std::sync::Arc;

use glam::Mat4;
use starfish_gfx::geometry::Geometry;
use starfish_gfx::pipeline::{line_pipeline_3d, shape_mesh};
use starfish_gfx::ShapeVertex;
use starfish_render::bind_group::bind_group::BindGroup;
use starfish_render::mesh::mesh::Mesh;
use starfish_render::pipeline::RenderPipeline;
use starfish_render::render_pass::render_pass::RenderPass;
use starfish_render::render_resource_access::RenderResourceAccess;
use wgpu::BufferUsages;

use super::sink::LineSink;

/// 物理调试线渲染器（3D；gfx 积木组装）。
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

        let pipeline = line_pipeline_3d(access, &camera_bind, &layout_mesh, sample_count.max(1));

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

    /// look-at + 透视相机（**3D**）：`fov_y_radians` = 垂直视场角，
    /// 视口宽高比定横向缩放；up 固定 (0,1,0)（debug 相机足够）。
    pub fn set_camera_lookat(
        &self,
        access: &RenderResourceAccess,
        eye: [f32; 3],
        target: [f32; 3],
        fov_y_radians: f32,
        viewport_px: (u32, u32),
    ) {
        let m = lookat_pvp(eye, target, fov_y_radians, viewport_px);
        access.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&m));
    }

    /// 相机 bind group（供宿主**自建管线共享同一相机**——shape.wgsl 契约：
    /// group0/binding0 = 64B mat4 MVP）。
    pub fn camera_bind(&self) -> &BindGroup {
        &self.camera_bind
    }

    /// 编码绘制（语义见 dim2 同名文档；索引数由 sink 实时决定）。
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

/// look-at 视图 × 透视投影（列主序，wgpu NDC，深度 0..1）。
///
/// 手写矩阵（不触 glam 的 deprecated 投影 API）；view×proj 乘法借 glam
/// Mat4。**公开**——宿主自建管线可复用同一相机数学。
pub fn lookat_pvp(
    eye: [f32; 3],
    target: [f32; 3],
    fov_y_radians: f32,
    viewport_px: (u32, u32),
) -> [[f32; 4]; 4] {
    use glam::Vec3;

    let (w, h) = (viewport_px.0.max(1) as f32, viewport_px.1.max(1) as f32);
    let aspect = w / h;
    let near = 0.1_f32;
    let far = 500.0_f32;

    // 视图（RH look-at；up = (0,1,0)）
    let eye = Vec3::from(eye);
    let target = Vec3::from(target);
    let fwd = (target - eye).normalize_or_zero();
    let right = fwd.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(fwd);
    let view = [
        [right.x, up.x, -fwd.x, 0.0],
        [right.y, up.y, -fwd.y, 0.0],
        [right.z, up.z, -fwd.z, 0.0],
        [-right.dot(eye), -up.dot(eye), fwd.dot(eye), 1.0],
    ];

    // 透视（RH，wgpu 深度 0..1）
    let f = 1.0 / fov_y_radians.tan();
    let proj = [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, near * far / (near - far), 0.0],
    ];

    // MVP = proj × view（列主序语义；乘法借 glam）
    let mvp = Mat4::from_cols_array_2d(&proj) * Mat4::from_cols_array_2d(&view);
    mvp.to_cols_array_2d()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3D 相机：轴上 eye 看原点 → 原点落在 NDC 中心（x=y=0）、深度 ∈(0,1)
    #[test]
    fn lookat透视_原点居中且深度合法() {
        let m = lookat_pvp([0.0, 0.0, 10.0], [0.0, 0.0, 0.0], 1.0, (800, 600));
        // 列主序 [col][row]：p=(0,0,0,1) 的 clip 各分量在平移列（col 3）的
        // 行 0..3 —— x/y/z/w = m[3][0..4]
        let (x, y, z, w) = (m[3][0], m[3][1], m[3][2], m[3][3]);
        let (x, y, z) = (x / w, y / w, z / w);
        assert!(x.abs() < 1e-5 && y.abs() < 1e-5, "原点应在 NDC 中心，实际 ({x},{y})");
        assert!(z > 0.0 && z < 1.0, "深度应 ∈(0,1)，实际 {z}");
        // 垂直缩放 f = 1/tan(fov/2)；横/纵比 = 800/600（对角元 m[0][0]/m[1][1]）
        let f = 1.0 / 1.0f32.tan();
        assert!((m[1][1] / m[0][0] - 800.0 / 600.0).abs() < 1e-4);
        assert!((m[0][0] - f / (800.0 / 600.0)).abs() < 1e-4);
    }

    /// 3D 相机：原点在视锥外（背后）→ 深度越界（调用方自行取舍）
    #[test]
    fn lookat透视_目标在背后越界() {
        let m = lookat_pvp([0.0, 0.0, 10.0], [0.0, 0.0, 100.0], 1.0, (800, 600));
        let z = m[3][2] / m[3][3];
        assert!(z < 0.0 || z > 1.0, "背后目标深度应越界，实际 {z}");
    }
}
