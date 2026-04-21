use nalgebra::{Matrix4, Vector3};

pub struct Camera {
    pub position: Vector3<f64>,
    pub rotation: Vector3<f64>,
    pub extent: [f64; 2],
    pub fov: f64,
}

/// Result of projecting a 3D world position to 2D screen coordinates.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ScreenProjection {
    /// Screen X coordinate (0 = left, extent[0] = right).
    pub x: f32,
    /// Screen Y coordinate (0 = top, extent[1] = bottom).
    pub y: f32,
    /// Whether the point is in front of the camera (positive Z in view space).
    pub in_front: bool,
    /// Distance from camera to the point.
    pub distance: f32,
}

impl Camera {
    pub fn new(position: Vector3<f64>, rotation: Vector3<f64>, extent: [f64; 2], fov: f64) -> Self {
        Camera {
            position,
            rotation,
            extent,
            fov,
        }
    }

    pub fn look_direction(&mut self, direction: Vector3<f64>) {
        // Y-up coordinate system: Y is vertical, -Z is forward
        // Yaw (rotation around Y/vertical axis) - left/right
        let yaw = direction.x.atan2(-direction.z);
        // Pitch (rotation around X axis) - up/down
        let horizontal_dist = (direction.x * direction.x + direction.z * direction.z).sqrt();
        let pitch = direction.y.atan2(horizontal_dist);
        self.rotation = Vector3::new(pitch, yaw, 0.0);
    }

    pub fn look_at(&mut self, target: Vector3<f64>) {
        let direction = target - self.position;
        self.look_direction(direction);
    }

    pub fn rotation_matrix(&self) -> Matrix4<f64> {
        // Y-up coordinate system: pitch around X, yaw around Y
        let pitch = Matrix4::from_axis_angle(&Vector3::x_axis(), self.rotation.x);
        let yaw = Matrix4::from_axis_angle(&Vector3::y_axis(), self.rotation.y);

        yaw * pitch
    }

    pub fn translation_matrix(&self) -> Matrix4<f64> {
        let mut translation = Matrix4::identity();
        translation.m14 = self.position.x;
        translation.m24 = self.position.y;
        translation.m34 = self.position.z;
        translation
    }

    pub fn pixel_to_ray_matrix(&self) -> Matrix4<f64> {
        let aspect = self.extent[0] / self.extent[1];
        let tan_fov = (self.fov.to_radians() * 0.5).tan();

        let mut center_pixel = Matrix4::identity();
        center_pixel.m13 = 0.5;
        center_pixel.m23 = 0.5;

        let mut pixel_to_uv = Matrix4::identity();
        pixel_to_uv.m11 = 2.0 / self.extent[0];
        pixel_to_uv.m22 = -2.0 / self.extent[1];
        pixel_to_uv.m13 = -1.0;
        pixel_to_uv.m23 = 1.0;

        let mut uv_to_view = Matrix4::identity();
        uv_to_view.m11 = tan_fov * aspect.max(1.0);
        uv_to_view.m22 = tan_fov / aspect.min(1.0);

        // Y-up coordinate system: negate Z to make forward be -Z
        let mut negate_z = Matrix4::identity();
        negate_z.m33 = -1.0;

        let rotation = self.rotation_matrix();
        let translation = self.translation_matrix();

        translation * rotation * negate_z * uv_to_view * pixel_to_uv * center_pixel
    }

    /// Projects a 3D world position to 2D screen coordinates.
    ///
    /// Returns screen coordinates where (0,0) is top-left and (width,height) is bottom-right.
    /// Also returns whether the point is in front of the camera and the distance.
    #[allow(dead_code)]
    pub fn world_to_screen(&self, world_pos: Vector3<f64>) -> ScreenProjection {
        let aspect = self.extent[0] / self.extent[1];
        let tan_fov = (self.fov.to_radians() * 0.5).tan();

        // Transform world position to view space (camera-relative)
        // First translate by negative camera position
        let relative = world_pos - self.position;

        // Apply inverse rotation manually
        // Y-up coordinate system: pitch around X, yaw around Y
        let (sin_pitch, cos_pitch) = (self.rotation.x.sin(), self.rotation.x.cos());
        let (sin_yaw, cos_yaw) = (self.rotation.y.sin(), self.rotation.y.cos());

        // Inverse yaw rotation (negate angle = transpose)
        let x1 = relative.x * cos_yaw + relative.z * sin_yaw;
        let z1 = -relative.x * sin_yaw + relative.z * cos_yaw;
        let y1 = relative.y;

        // Inverse pitch rotation
        let y2 = y1 * cos_pitch + z1 * sin_pitch;
        let z2 = -y1 * sin_pitch + z1 * cos_pitch;
        let x2 = x1;

        // In our coordinate system, forward is -Z, so we check if z < 0 for "in front"
        let in_front = z2 < 0.0;

        // Calculate distance from camera
        let distance = relative.magnitude() as f32;

        // Project to screen coordinates
        // At tan_fov, z=-1 maps to y=±tan_fov in NDC
        // We need to handle the case where z is near zero
        let z = -z2; // Negate because forward is -Z
        let screen_x =
            self.extent[0] / 2.0 + (x2 / z) * (self.extent[0] / 2.0) / (tan_fov * aspect.max(1.0));
        let screen_y = self.extent[1] / 2.0 - (y2 / z) * (self.extent[1] / 2.0) / tan_fov;

        ScreenProjection {
            x: screen_x as f32,
            y: screen_y as f32,
            in_front,
            distance,
        }
    }
}
