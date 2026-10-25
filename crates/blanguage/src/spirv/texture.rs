//! GPU 采样直接发出标准 sampled-image 指令，使用显式 LOD。
use super::*;
impl Emitter<'_> {
    fn sampled_texture(&mut self, texture: TextureSource) -> u32 {
        let mut pointer = self.textures[texture.binding];
        if let Some(element) = texture.element {
            let index = self.id();
            self.emit(83, &[self.types[&Scalar::U32], index, self.value(element)]);
            op(&mut self.decorations, 71, &[index, 5300]);
            let selected = self.id();
            self.emit(65, &[self.sampled_pointer, selected, pointer, index]);
            op(&mut self.decorations, 71, &[selected, 5300]);
            pointer = selected;
        }
        let sampled = self.id();
        self.emit(61, &[self.sampled_type, sampled, pointer]);
        if texture.element.is_some() {
            op(&mut self.decorations, 71, &[sampled, 5300]);
        }
        sampled
    }
    pub(super) fn texture_image(&mut self, texture: TextureSource) -> u32 {
        let sampled = self.sampled_texture(texture);
        let image = self.id();
        self.emit(100, &[self.image_type, image, sampled]);
        if texture.element.is_some() {
            op(&mut self.decorations, 71, &[image, 5300]);
        }
        image
    }
    pub(super) fn texture_sample(
        &mut self,
        result: [Reg; 4],
        texture: TextureSource,
        coordinate: [Reg; 2],
        lod: Reg,
    ) {
        let sampled = self.sampled_texture(texture);
        let coord = self.id();
        self.emit(
            80,
            &[
                self.vec2_type,
                coord,
                self.value(coordinate[0]),
                self.value(coordinate[1]),
            ],
        );
        let value = self.id();
        self.emit(
            88,
            &[self.vec4_type, value, sampled, coord, 2, self.value(lod)],
        );
        for (i, r) in result.iter().enumerate() {
            self.emit(81, &[self.ty(*r), self.value(*r), value, i as u32]);
        }
    }
}
