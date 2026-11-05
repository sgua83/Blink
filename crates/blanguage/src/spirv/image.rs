//! 已编译GPU映像及页绑定ABI；编译器与执行器消费同一份表示。
use crate::ir::ExecutionPlan;

/// GPU 存储页的最大字节数；每页只容纳完整记录，页表使用 64 位设备地址。
pub const PAGE_BYTES: u32 = 256 * 1024 * 1024;
/// 页表绑定头为页数组地址、物理stride和字段offset；GPU view仍指向页数组。
pub const BINDING_BYTES: u32 = 16;

/// u32 元素域按 64 路分组递归至单组所需的最大层数。
pub const PREFIX_LEVELS: usize = u32::BITS.div_ceil(6) as usize;

#[derive(Clone, Debug)]
pub struct ShaderCode {
    pub words: Vec<u32>,
    pub ranks: bool,
}
#[derive(Clone, Debug)]
pub struct StageCode {
    pub main: Vec<u32>,
    pub tail: Option<Vec<u32>>,
    pub scan_add: Option<Vec<u32>>,
    pub finish: Option<(String, Vec<u32>)>,
    pub dispatch: Option<Vec<u32>>,
    pub selector: Option<Vec<u32>>,
    pub sort: Option<Vec<ShaderCode>>,
}
#[derive(Clone, Debug)]
pub struct ProgramImage<P> {
    program: std::sync::Arc<ExecutionPlan>,
    marker: std::marker::PhantomData<fn() -> P>,
    stages: Vec<StageCode>,
}
impl<P> ProgramImage<P> {
    /// 恢复本版本编译器输出的 SPIR-V 与执行布局。
    ///
    /// # Safety
    /// 必须来自本版本编译器的同一个 P 入口，包含完整未修改的 Shader、
    /// 调度、设备需求及前缀关系；不得由宿主输入构造或伪造条件模板的证明。
    #[doc(hidden)]
    pub unsafe fn from_compiler_output(program: impl Into<std::sync::Arc<ExecutionPlan>>, stages: Vec<StageCode>) -> Self {
        Self {
            program: program.into(),
            stages,
            marker: std::marker::PhantomData,
        }
    }
    pub fn program(&self) -> &ExecutionPlan {
        &self.program
    }
    pub fn shared_program(&self) -> std::sync::Arc<ExecutionPlan> {
        self.program.clone()
    }
    pub fn stages(&self) -> &[StageCode] {
        &self.stages
    }
    pub fn into_parts(self) -> (std::sync::Arc<ExecutionPlan>, Vec<StageCode>) {
        (self.program, self.stages)
    }
}
