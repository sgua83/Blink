#![allow(dead_code)]
use std::ffi::c_void;
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct Span{pub start:u64, pub end:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct Slice{pub offset:u64, pub count:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct Text{pub kind:u32, pub range:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct Optional<T>{pub present:u32, pub value:T,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct TypeRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ShapeRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ArgumentRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ConditionRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct PredicateRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ContractRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct TypeParameterRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ValueParameterRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct TypeFamilyRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct FieldRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct VariantRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct RecordRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ParamRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ExternalRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct FunctionRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct TypeSetRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ModuleRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ExprRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct StmtRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ParallelRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct ArmRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy,Default)]
pub struct IntervalRef{pub offset:u64,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct TypeNode{pub name:Text, pub canonical:u32, pub arguments:Slice, pub dataset:u32, pub view:u32, pub tile:u32, pub count:Optional<ShapeRef>, pub contracts:Optional<Slice>,}
#[repr(C,u32)]
#[derive(Clone,Copy)]
pub enum ShapeNode{Literal(u32)=0, Constant(Text)=1, Expression(ExprRef)=2,}
#[repr(C,u32)]
#[derive(Clone,Copy)]
pub enum ArgumentNode{Type(TypeRef)=0, Value(ShapeRef)=1,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ConditionMember{pub base:ConditionRef, pub field:Text,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ConditionUnary{pub operation:Text, pub value:ConditionRef,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ConditionBinary{pub operation:Text, pub left:ConditionRef, pub right:ConditionRef,}
#[repr(C,u32)]
#[derive(Clone,Copy)]
pub enum ConditionNode{Name(Text)=0, Number(Text)=1, Bool(u32)=2, Member(ConditionMember)=3, Unary(ConditionUnary)=4, Binary(ConditionBinary)=5, StaticU32(u32)=6,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct PrefixPredicate{pub data:Text, pub count:Text, pub field:Text,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct IndexPredicate{pub value:Text, pub resource:Text,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct RangePredicate{pub value:Text, pub intervals:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct SameLengthPredicate{pub left:Text, pub right:Text,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ConditionalPredicate{pub value:Text, pub condition:ConditionRef,}
#[repr(C,u32)]
#[derive(Clone,Copy)]
pub enum PredicateNode{Prefix(PrefixPredicate)=0, Nonzero(Text)=1, Nonempty(Text)=2, Index(IndexPredicate)=3, Contiguous(Text)=4, Initialized(ConditionalPredicate)=5, SameLength(SameLengthPredicate)=6, Cleanup(ConditionalPredicate)=7, Memory(Optional<crate::ast::Access>)=8, Range(RangePredicate)=9,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct IntervalNode{pub lower:ConditionRef, pub upper:ConditionRef,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ContractNode{pub kind:crate::ast::ContractKind, pub predicate:PredicateRef, pub span:Span,}
#[repr(C,u32)]
#[derive(Clone,Copy)]
pub enum TypeFamilyNode{Numeric=0, Floating=1, Integer=2, ScalarNumeric=3, ScalarInteger=4, Named(Text)=5,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct TypeParameterNode{pub name:Text, pub family:Optional<TypeFamilyRef>,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ValueParameterNode{pub name:Text, pub choices:Optional<Slice>,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct FieldNode{pub name:Text, pub ty:TypeRef, pub span:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct VariantNode{pub name:Text, pub payload:TypeRef, pub discriminant:ExprRef, pub span:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct RecordNode{pub name:Text, pub types:Slice, pub values:Slice, pub stride:Optional<ShapeRef>, pub enumeration:Optional<TypeRef>, pub variants:Slice, pub fields:Slice, pub span:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ParamNode{pub name:Text, pub ty:TypeRef, pub access:crate::ast::Access, pub span:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ExternalNode{pub library:Text, pub symbol:Text,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct FunctionNode{pub name:Text, pub types:Slice, pub values:Slice, pub params:Slice, pub result:TypeRef, pub kind:crate::ast::FunctionKind, pub target:crate::ast::Target, pub contracts:Slice, pub body:Optional<Slice>, pub c_abi:u32, pub external:Optional<ExternalRef>, pub signature_end:u64, pub span:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct TypeSetNode{pub name:Text, pub members:Slice, pub span:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ModuleNode{pub name:Text, pub imports:Slice, pub records:Slice, pub type_sets:Slice, pub functions:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct MemberExpr{pub base:ExprRef, pub field:Text,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct IndexExpr{pub base:ExprRef, pub index:ExprRef,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct CallExpr{pub name:Text, pub arguments:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct InvokeExpr{pub callee:ExprRef, pub arguments:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ConstructExpr{pub ty:TypeRef, pub arguments:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct SpecializeExpr{pub name:Text, pub types:Slice, pub arguments:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct FunctionExpr{pub name:Text, pub types:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct CooperateExpr{pub name:Text, pub types:Optional<Slice>, pub arguments:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct TargetExpr{pub target:crate::ast::Target, pub value:ExprRef,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct UnaryExpr{pub operation:Text, pub value:ExprRef,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct BinaryExpr{pub operation:Text, pub left:ExprRef, pub right:ExprRef,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct SelectExpr{pub condition:ExprRef, pub yes:ExprRef, pub no:ExprRef,}
#[repr(C,u32)]
#[derive(Clone,Copy)]
pub enum ExprKind{Number(Text)=0, StaticU32(u32)=1, Bool(u32)=2, Name(Text)=3, Member(MemberExpr)=4, Index(IndexExpr)=5, Call(CallExpr)=6, Invoke(InvokeExpr)=7, Construct(ConstructExpr)=8, Specialize(SpecializeExpr)=9, FunctionRef(FunctionExpr)=10, Cooperate(CooperateExpr)=11, Target(TargetExpr)=12, Unary(UnaryExpr)=13, Binary(BinaryExpr)=14, Select(SelectExpr)=15,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ExprNode{pub kind:ExprKind, pub span:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct DeclareStmt{pub ty:TypeRef, pub name:Text,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct LetStmt{pub annotation:Optional<TypeRef>, pub name:Text, pub value:ExprRef,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct AssignStmt{pub target:ExprRef, pub operation:Text, pub value:ExprRef,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct IfStmt{pub condition:ExprRef, pub yes:Slice, pub no:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct MatchStmt{pub value:ExprRef, pub arms:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct WhileStmt{pub condition:ExprRef, pub body:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ForStmt{pub name:Text, pub start:ExprRef, pub end:Optional<ExprRef>, pub body:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ParallelStmt{pub name:Text, pub domain:ParallelRef, pub body:Slice,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct SeekStmt{pub name:Text, pub value:ExprRef,}
#[repr(C,u32)]
#[derive(Clone,Copy)]
pub enum StmtKind{Declare(DeclareStmt)=0, Let(LetStmt)=1, Assign(AssignStmt)=2, Expr(ExprRef)=3, If(IfStmt)=4, Match(MatchStmt)=5, While(WhileStmt)=6, For(ForStmt)=7, Parallel(ParallelStmt)=8, Return(Optional<ExprRef>)=9, Break=10, Continue=11, Phase=12, Seek(SeekStmt)=13,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct StmtNode{pub kind:StmtKind, pub span:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ParallelNode{pub dataset:Text, pub count:Optional<ExprRef>, pub selection:Optional<ExprRef>,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ArmNode{pub variant:Text, pub binding:Optional<Text>, pub body:Slice, pub span:Span,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct StorageError{pub kind:u32, pub native_code:u32,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct StorageCreate{pub storage:*mut c_void, pub error:StorageError,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct StorageIndex{pub offset:u64, pub error:StorageError,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct StorageSlice{pub value:Slice, pub error:StorageError,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct StorageText{pub value:Text, pub error:StorageError,}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct StorageView{pub nodes:*mut c_void, pub node_bytes:u64, pub text:*mut c_void, pub text_bytes:u64, pub source:*mut c_void, pub source_bytes:u64,}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SignatureNode {
    pub storage: StorageView,
    pub offset: u64,
}

#[repr(C)]
#[derive(Clone,Copy)]
pub struct StorageCondition{pub value:ConditionRef, pub present:u32, pub error:StorageError,}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InstanceDomains {
    pub nodes: *const c_void,
    pub parameters: Slice,
    pub type_sizes: *const u64,
    pub type_count: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InstanceRow {
    pub indices: *mut u64,
    pub values: *mut u32,
}
#[repr(C, u32)]
pub enum RecordMember {
    Missing = 0,
    Variant = 1,
    Field(TypeRef) = 2,
}

unsafe extern "C" {
    #[link_name = "bootstrap.signatures::record_member"]
    pub fn record_member(record: SignatureNode, name: *const u8, length: u64) -> RecordMember;
    #[link_name = "bootstrap.signatures::compare_condition"]
    pub fn compare_condition(left: SignatureNode, right: SignatureNode) -> i32;
    #[link_name = "bootstrap.signatures::compare_predicate"]
    pub fn compare_predicate(left: SignatureNode, right: SignatureNode) -> i32;
    #[link_name = "bootstrap.signatures::same_signature"]
    pub fn same_signature(left: SignatureNode, right: SignatureNode, left_contracts: *mut u64, right_contracts: *mut u64) -> bool;
}
unsafe extern "C" {
#[link_name="bootstrap.ast_storage::create"] pub fn create(source:*mut c_void,length:u64)->StorageCreate;
#[link_name="bootstrap.ast_storage::destroy"] pub fn destroy(storage:*mut c_void)->StorageError;
#[link_name="bootstrap.ast_storage::view"] pub fn view(storage:*mut c_void)->StorageView;
#[link_name="bootstrap.instance_domain::advance"]
pub fn advance_instance_domain(domains:InstanceDomains, row:InstanceRow, first:u32)->u32;
#[link_name="bootstrap.ast_storage::indices"] pub fn indices(storage:*mut c_void,values:*mut c_void,count:u64)->StorageSlice;
#[link_name="bootstrap.ast_storage::choices"] pub fn choices(storage:*mut c_void,values:*mut c_void,count:u64)->StorageSlice;
#[link_name="bootstrap.ast_storage::texts"] pub fn texts(storage:*mut c_void,values:*mut c_void,count:u64)->StorageSlice;
#[link_name="bootstrap.ast_storage::text"] pub fn text(storage:*mut c_void,bytes:*mut c_void,count:u64)->StorageText;
#[link_name="bootstrap.ast_storage::put_type"] pub fn put_type(storage:*mut c_void,value:*const TypeNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_shape"] pub fn put_shape(storage:*mut c_void,value:*const ShapeNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_argument"] pub fn put_argument(storage:*mut c_void,value:*const ArgumentNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_condition"] pub fn put_condition(storage:*mut c_void,value:*const ConditionNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_predicate"] pub fn put_predicate(storage:*mut c_void,value:*const PredicateNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_interval"] pub fn put_interval(storage:*mut c_void,value:*const IntervalNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_contract"] pub fn put_contract(storage:*mut c_void,value:*const ContractNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_type_family"] pub fn put_type_family(storage:*mut c_void,value:*const TypeFamilyNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_type_parameter"] pub fn put_type_parameter(storage:*mut c_void,value:*const TypeParameterNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_value_parameter"] pub fn put_value_parameter(storage:*mut c_void,value:*const ValueParameterNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_field"] pub fn put_field(storage:*mut c_void,value:*const FieldNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_variant"] pub fn put_variant(storage:*mut c_void,value:*const VariantNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_record"] pub fn put_record(storage:*mut c_void,value:*const RecordNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_param"] pub fn put_param(storage:*mut c_void,value:*const ParamNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_external"] pub fn put_external(storage:*mut c_void,value:*const ExternalNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_function"] pub fn put_function(storage:*mut c_void,value:*const FunctionNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_type_set"] pub fn put_type_set(storage:*mut c_void,value:*const TypeSetNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_module"] pub fn put_module(storage:*mut c_void,value:*const ModuleNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_expr"] pub fn put_expr(storage:*mut c_void,value:*const ExprNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_stmt"] pub fn put_stmt(storage:*mut c_void,value:*const StmtNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_parallel"] pub fn put_parallel(storage:*mut c_void,value:*const ParallelNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::put_arm"] pub fn put_arm(storage:*mut c_void,value:*const ArmNode)->StorageIndex;
#[link_name="bootstrap.ast_storage::condition"] pub fn condition(storage:*mut c_void,expression:ExprRef)->StorageCondition;
}

unsafe extern "C" {#[link_name="bootstrap.ast_storage::condition_input"] pub fn condition_input(storage:*mut c_void,node:*const ExprNode)->StorageCondition;}
impl<T:Default> Optional<T>{pub fn none()->Self{Self{present:0,value:T::default()}}}
impl<T> Optional<T>{pub fn some(value:T)->Self{Self{present:1,value}}}
