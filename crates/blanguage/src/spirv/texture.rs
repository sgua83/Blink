//! 从原纹理绑定解析采样所用的类型与值，交接原 SPIR-V 字流。
use super::*;

impl Emitter<'_> {
    pub(super) fn texture_sample(
        &mut self,
        result: [Reg; 4],
        texture: TextureSource,
        coordinate: &[Reg],
        lod: Reg,
    ) {
        use crate::ast::TextureDimension;
        let dimension = self.program.textures[texture.binding].dimension;
        let (sampled_type, sampled_pointer) = self.texture_types[&dimension];
        let input = TextureSampleEmission {
            variable: self.textures[texture.binding],
            element: texture.element.map_or(0, |reg| self.value(reg)),
            index_type: self.types[&Scalar::U32],
            sampled_type,
            sampled_pointer,
            coordinate_type: match dimension {
                TextureDimension::D2 => self.vec2_type,
                TextureDimension::D3 => self.vec3_type,
            },
            vector_type: self.vec4_type,
            component_type: self.types[&Scalar::F32],
            x: self.value(coordinate[0]),
            y: self.value(coordinate[1]),
            z: coordinate.get(2).map_or(0, |reg| self.value(*reg)),
            lod: self.value(lod),
            next: self.next,
        };
        let result = result.map(|reg| self.value(reg));
        let body_at = self.body.len();
        let decoration_at = self.decorations.len();
        self.body.reserve(46);
        self.decorations.reserve(9);
        unsafe {
            let emitted = encode_texture_sample(
                input,
                result.as_ptr(),
                self.body.as_mut_ptr().add(body_at),
                self.decorations.as_mut_ptr().add(decoration_at),
            );
            self.body.set_len(body_at + emitted.body as usize);
            self.decorations.set_len(decoration_at + emitted.decorations as usize);
            self.next = emitted.next;
        }
    }
}
