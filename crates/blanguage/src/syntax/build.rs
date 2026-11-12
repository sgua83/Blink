//! Construction values contain only the current node and already stored child indices.
use super::{wire,Builder,Result};
use crate::ast::{Access,ContractKind,FunctionKind,Span,Target,TypeFamily};

#[derive(Clone)]
pub(crate) struct TypeName {
    pub name:String,pub canonical:bool,pub arguments:Vec<wire::ArgumentRef>,
    pub dataset:bool,pub view:bool,pub tile:bool,pub count:Option<Shape>,
    pub contracts:Option<Vec<wire::PredicateRef>>
}
impl TypeName {pub fn named(name:impl Into<String>)->Self {Self{name:name.into(),canonical:false,arguments:Vec::new(),dataset:false,view:false,tile:false,count:None,contracts:None}}}
#[derive(Clone)]
pub(crate) enum Shape {Literal(u32),Constant(String),Expression(wire::ExprRef)}
pub(crate) enum GenericArgument {Type(TypeName),Value(Shape)}
pub(crate) enum Predicate {
    Prefix{data:String,count:String,field:String},Nonzero(String),Nonempty(String),
    Contiguous(String),Memory(Option<Access>),Index{value:String,resource:String},
    Range{value:String,intervals:Vec<(wire::ConditionRef,wire::ConditionRef)>},
    SameLength{left:String,right:String},Initialized{value:String,condition:wire::ConditionRef},
    Cleanup{value:String,condition:wire::ConditionRef}
}
pub(crate) struct Contract {pub kind:ContractKind,pub predicate:Predicate,pub span:Span}
pub(crate) struct TypeParameter {pub name:String,pub family:Option<TypeFamily>}
pub(crate) struct ValueParameter {pub name:String,pub choices:Option<Vec<u32>>}
pub(crate) struct Param {pub name:String,pub ty:TypeName,pub access:Access,pub span:Span}
pub(crate) struct Field {pub name:String,pub ty:TypeName,pub span:Span}
pub(crate) struct VariantDeclaration {pub name:String,pub payload:TypeName,pub discriminant:wire::ExprRef,pub span:Span}
pub(crate) struct Record {
    pub name:String,pub types:Vec<wire::TypeParameterRef>,pub values:Vec<wire::ValueParameterRef>,
    pub stride:Option<Shape>,pub enumeration:Option<TypeName>,pub variants:Vec<wire::VariantRef>,
    pub fields:Vec<wire::FieldRef>,pub span:Span
}
pub(crate) struct TypeSet {pub name:String,pub members:Vec<wire::TypeRef>,pub span:Span}
pub(crate) struct Function {
    pub name:String,pub types:Vec<wire::TypeParameterRef>,pub values:Vec<wire::ValueParameterRef>,
    pub params:Vec<wire::ParamRef>,pub result:TypeName,pub kind:FunctionKind,pub target:Target,
    pub contracts:Vec<wire::ContractRef>,pub body:Option<Vec<wire::StmtRef>>,pub c_abi:bool,
    pub external:Option<crate::ast::ExternalName>,pub signature_end:usize,pub span:Span
}
pub(crate) struct Module {
    pub name:String,pub imports:Vec<String>,pub records:Vec<wire::RecordRef>,
    pub type_sets:Vec<wire::TypeSetRef>,pub functions:Vec<wire::FunctionRef>
}
pub(crate) struct Expr {pub kind:ExprKind,pub span:Span}
pub(crate) enum ExprKind {
    Number(String),StaticU32(u32),Bool(bool),Name(String),Member(wire::ExprRef,String),
    Index(wire::ExprRef,wire::ExprRef),Call(String,Vec<wire::ExprRef>),Invoke(wire::ExprRef,Vec<wire::ExprRef>),
    Construct(TypeName,Vec<wire::ExprRef>),Specialize(String,Vec<wire::ArgumentRef>,Vec<wire::ExprRef>),
    FunctionRef(String,Vec<wire::ArgumentRef>),Cooperate(String,Option<Vec<wire::ArgumentRef>>,Vec<wire::ExprRef>),
    Target(Target,wire::ExprRef),Unary(String,wire::ExprRef),Binary(String,wire::ExprRef,wire::ExprRef),
    Select(wire::ExprRef,wire::ExprRef,wire::ExprRef)
}
pub(crate) struct Stmt {pub kind:StmtKind,pub span:Span}
pub(crate) struct ParallelDomain {pub dataset:String,pub count:Option<wire::ExprRef>,pub selection:Option<wire::ExprRef>}
pub(crate) struct MatchArm {pub variant:String,pub binding:Option<String>,pub body:Vec<wire::StmtRef>,pub span:Span}
pub(crate) enum StmtKind {
    Declare(TypeName,String),Let(Option<TypeName>,String,wire::ExprRef),Assign(wire::ExprRef,String,wire::ExprRef),
    Expr(wire::ExprRef),If(wire::ExprRef,Vec<wire::StmtRef>,Vec<wire::StmtRef>),Match(wire::ExprRef,Vec<wire::ArmRef>),
    While(wire::ExprRef,Vec<wire::StmtRef>),For(String,wire::ExprRef,Option<wire::ExprRef>,Vec<wire::StmtRef>),
    Parallel(String,ParallelDomain,Vec<wire::StmtRef>),Return(Option<wire::ExprRef>),Break,Continue,Phase,Seek(String,wire::ExprRef)
}
fn span(value:Span)->wire::Span {wire::Span{start:value.start as u64,end:value.end as u64}}
/// All implementations are private repr(C) rows containing exactly one u64 offset.
pub(crate) unsafe trait Reference:Copy {}
unsafe impl Reference for wire::TypeRef {}
unsafe impl Reference for wire::ShapeRef {}
unsafe impl Reference for wire::ArgumentRef {}
unsafe impl Reference for wire::ConditionRef {}
unsafe impl Reference for wire::PredicateRef {}
unsafe impl Reference for wire::ContractRef {}
unsafe impl Reference for wire::TypeParameterRef {}
unsafe impl Reference for wire::ValueParameterRef {}
unsafe impl Reference for wire::TypeFamilyRef {}
unsafe impl Reference for wire::FieldRef {}
unsafe impl Reference for wire::VariantRef {}
unsafe impl Reference for wire::RecordRef {}
unsafe impl Reference for wire::ParamRef {}
unsafe impl Reference for wire::ExternalRef {}
unsafe impl Reference for wire::FunctionRef {}
unsafe impl Reference for wire::TypeSetRef {}
unsafe impl Reference for wire::ModuleRef {}
unsafe impl Reference for wire::ExprRef {}
unsafe impl Reference for wire::StmtRef {}
unsafe impl Reference for wire::ParallelRef {}
unsafe impl Reference for wire::ArmRef {}
unsafe impl Reference for wire::IntervalRef {}

impl Builder {
    pub fn refs<T:Reference>(&mut self,values:&[T])->Result<wire::Slice> {
        let result=unsafe{wire::indices(self.owner.raw.as_ptr(),values.as_ptr().cast_mut().cast(),values.len()as u64)};
        super::checked(result.error)?;Ok(result.value)
    }
    fn optional_type(&mut self,value:Option<TypeName>)->Result<wire::Optional<wire::TypeRef>> {
        Ok(match value{Some(v)=>wire::Optional::some(self.make_type(v)?),None=>wire::Optional::none()})
    }
    fn optional_shape(&mut self,value:Option<Shape>)->Result<wire::Optional<wire::ShapeRef>> {
        Ok(match value{Some(v)=>wire::Optional::some(self.make_shape(v)?),None=>wire::Optional::none()})
    }
    fn optional_refs<T:Reference>(&mut self,value:Option<Vec<T>>)->Result<wire::Optional<wire::Slice>> {
        Ok(match value{Some(v)=>wire::Optional::some(self.refs(&v)?),None=>wire::Optional::none()})
    }
    pub fn make_type(&mut self,v:TypeName)->Result<wire::TypeRef> {
        let row=wire::TypeNode{name:self.text(&v.name)?,canonical:u32::from(v.canonical),
            arguments:self.refs(&v.arguments)?,dataset:u32::from(v.dataset),view:u32::from(v.view),tile:u32::from(v.tile),
            count:self.optional_shape(v.count)?,contracts:self.optional_refs(v.contracts)?};
        Ok(wire::TypeRef{offset:self.put_type(&row)?})
    }
    pub fn make_shape(&mut self,v:Shape)->Result<wire::ShapeRef> {
        let row=match v {Shape::Literal(v)=>wire::ShapeNode::Literal(v),
            Shape::Constant(v)=>wire::ShapeNode::Constant(self.text(&v)?),
            Shape::Expression(v)=>wire::ShapeNode::Expression(v)};
        Ok(wire::ShapeRef{offset:self.put_shape(&row)?})
    }
    pub fn make_argument(&mut self,v:GenericArgument)->Result<wire::ArgumentRef> {
        let row=match v{GenericArgument::Type(v)=>wire::ArgumentNode::Type(self.make_type(v)?),
            GenericArgument::Value(v)=>wire::ArgumentNode::Value(self.make_shape(v)?)};
        Ok(wire::ArgumentRef{offset:self.put_argument(&row)?})
    }
    fn expr_row(&mut self,v:Expr)->Result<wire::ExprNode> {
        use ExprKind as K;use wire::ExprKind as W;
        let kind=match v.kind {
            K::Number(v)=>W::Number(self.text(&v)?),K::StaticU32(v)=>W::StaticU32(v),K::Bool(v)=>W::Bool(u32::from(v)),K::Name(v)=>W::Name(self.text(&v)?),
            K::Member(base,field)=>W::Member(wire::MemberExpr{base,field:self.text(&field)?}),
            K::Index(base,index)=>W::Index(wire::IndexExpr{base,index}),
            K::Call(name,args)=>W::Call(wire::CallExpr{name:self.text(&name)?,arguments:self.refs(&args)?}),
            K::Invoke(callee,args)=>W::Invoke(wire::InvokeExpr{callee,arguments:self.refs(&args)?}),
            K::Construct(ty,args)=>W::Construct(wire::ConstructExpr{ty:self.make_type(ty)?,arguments:self.refs(&args)?}),
            K::Specialize(name,types,args)=>W::Specialize(wire::SpecializeExpr{name:self.text(&name)?,types:self.refs(&types)?,arguments:self.refs(&args)?}),
            K::FunctionRef(name,types)=>W::FunctionRef(wire::FunctionExpr{name:self.text(&name)?,types:self.refs(&types)?}),
            K::Cooperate(name,types,args)=>W::Cooperate(wire::CooperateExpr{name:self.text(&name)?,types:self.optional_refs(types)?,arguments:self.refs(&args)?}),
            K::Target(target,value)=>W::Target(wire::TargetExpr{target,value}),
            K::Unary(op,value)=>W::Unary(wire::UnaryExpr{operation:self.text(&op)?,value}),
            K::Binary(op,left,right)=>W::Binary(wire::BinaryExpr{operation:self.text(&op)?,left,right}),
            K::Select(condition,yes,no)=>W::Select(wire::SelectExpr{condition,yes,no})
        };
        Ok(wire::ExprNode{kind,span:span(v.span)})
    }
    pub fn make_expr(&mut self,v:Expr)->Result<wire::ExprRef> {
        let row=self.expr_row(v)?;Ok(wire::ExprRef{offset:self.put_expr(&row)?})
    }
    pub fn source_condition(&mut self,v:Expr)->Result<Option<wire::ConditionRef>> {
        let row=self.expr_row(v)?;let result=unsafe{wire::condition_input(self.owner.raw.as_ptr(),&row)};
        super::checked(result.error)?;Ok((result.present!=0).then_some(result.value))
    }
    pub fn make_predicate(&mut self,v:Predicate)->Result<wire::PredicateRef> {
        use Predicate as K;use wire::PredicateNode as W;
        let row=match v{
            K::Prefix{data,count,field}=>W::Prefix(wire::PrefixPredicate{data:self.text(&data)?,count:self.text(&count)?,field:self.text(&field)?}),
            K::Nonzero(v)=>W::Nonzero(self.text(&v)?),K::Nonempty(v)=>W::Nonempty(self.text(&v)?),K::Contiguous(v)=>W::Contiguous(self.text(&v)?),
            K::Memory(v)=>W::Memory(match v{Some(v)=>wire::Optional::some(v),None=>wire::Optional{present:0,value:Access::Value}}),
            K::Index{value,resource}=>W::Index(wire::IndexPredicate{value:self.text(&value)?,resource:self.text(&resource)?}),
            K::Range{value,intervals}=>{
                let mut refs=Vec::with_capacity(intervals.len());
                for(lower,upper)in intervals{refs.push(wire::IntervalRef{offset:self.put_interval(&wire::IntervalNode{lower,upper})?});}
                W::Range(wire::RangePredicate{value:self.text(&value)?,intervals:self.refs(&refs)?})
            }
            K::SameLength{left,right}=>W::SameLength(wire::SameLengthPredicate{left:self.text(&left)?,right:self.text(&right)?}),
            K::Initialized{value,condition}=>W::Initialized(wire::ConditionalPredicate{value:self.text(&value)?,condition}),
            K::Cleanup{value,condition}=>W::Cleanup(wire::ConditionalPredicate{value:self.text(&value)?,condition})
        };
        Ok(wire::PredicateRef{offset:self.put_predicate(&row)?})
    }
    pub fn make_contract(&mut self,v:Contract)->Result<wire::ContractRef> {
        let row=wire::ContractNode{kind:v.kind,predicate:self.make_predicate(v.predicate)?,span:span(v.span)};
        Ok(wire::ContractRef{offset:self.put_contract(&row)?})
    }
    pub fn make_type_parameter(&mut self,v:TypeParameter)->Result<wire::TypeParameterRef> {
        let family=match v.family{
            None=>wire::Optional::none(),
            Some(v)=>{
                let row=match v{
                    TypeFamily::Numeric=>wire::TypeFamilyNode::Numeric,TypeFamily::Floating=>wire::TypeFamilyNode::Floating,
                    TypeFamily::Integer=>wire::TypeFamilyNode::Integer,TypeFamily::ScalarNumeric=>wire::TypeFamilyNode::ScalarNumeric,
                    TypeFamily::ScalarInteger=>wire::TypeFamilyNode::ScalarInteger,TypeFamily::Named(v)=>wire::TypeFamilyNode::Named(self.text(&v)?)
                };
                wire::Optional::some(wire::TypeFamilyRef{offset:self.put_type_family(&row)?})
            }
        };
        let row=wire::TypeParameterNode{name:self.text(&v.name)?,family};
        Ok(wire::TypeParameterRef{offset:self.put_type_parameter(&row)?})
    }
    pub fn make_value_parameter(&mut self,v:ValueParameter)->Result<wire::ValueParameterRef> {
        let choices=match v.choices{Some(mut v)=>wire::Optional::some(self.choices(&mut v)?),None=>wire::Optional::none()};
        let row=wire::ValueParameterNode{name:self.text(&v.name)?,choices};
        Ok(wire::ValueParameterRef{offset:self.put_value_parameter(&row)?})
    }
    pub fn make_param(&mut self,v:Param)->Result<wire::ParamRef> {
        let row=wire::ParamNode{name:self.text(&v.name)?,ty:self.make_type(v.ty)?,access:v.access,span:span(v.span)};
        Ok(wire::ParamRef{offset:self.put_param(&row)?})
    }
    pub fn make_field(&mut self,v:Field)->Result<wire::FieldRef> {
        let row=wire::FieldNode{name:self.text(&v.name)?,ty:self.make_type(v.ty)?,span:span(v.span)};
        Ok(wire::FieldRef{offset:self.put_field(&row)?})
    }
    pub fn make_variant(&mut self,v:VariantDeclaration)->Result<wire::VariantRef> {
        let row=wire::VariantNode{name:self.text(&v.name)?,payload:self.make_type(v.payload)?,discriminant:v.discriminant,span:span(v.span)};
        Ok(wire::VariantRef{offset:self.put_variant(&row)?})
    }
    pub fn make_record(&mut self,v:Record)->Result<wire::RecordRef> {
        let row=wire::RecordNode{name:self.text(&v.name)?,types:self.refs(&v.types)?,values:self.refs(&v.values)?,
            stride:self.optional_shape(v.stride)?,enumeration:self.optional_type(v.enumeration)?,
            variants:self.refs(&v.variants)?,fields:self.refs(&v.fields)?,span:span(v.span)};
        Ok(wire::RecordRef{offset:self.put_record(&row)?})
    }
    pub fn make_type_set(&mut self,v:TypeSet)->Result<wire::TypeSetRef> {
        let row=wire::TypeSetNode{name:self.text(&v.name)?,members:self.refs(&v.members)?,span:span(v.span)};
        Ok(wire::TypeSetRef{offset:self.put_type_set(&row)?})
    }
    pub fn make_function(&mut self,v:Function)->Result<wire::FunctionRef> {
        let external=match v.external{
            Some(v)=>{
                let row=wire::ExternalNode{library:self.text(&v.library)?,symbol:self.text(&v.symbol)?};
                wire::Optional::some(wire::ExternalRef{offset:self.put_external(&row)?})
            },None=>wire::Optional::none()
        };
        let row=wire::FunctionNode{name:self.text(&v.name)?,types:self.refs(&v.types)?,values:self.refs(&v.values)?,params:self.refs(&v.params)?,
            result:self.make_type(v.result)?,kind:v.kind,target:v.target,contracts:self.refs(&v.contracts)?,
            body:self.optional_refs(v.body)?,c_abi:u32::from(v.c_abi),external,signature_end:v.signature_end as u64,span:span(v.span)};
        Ok(wire::FunctionRef{offset:self.put_function(&row)?})
    }
    pub fn make_module(&mut self,v:Module)->Result<wire::ModuleRef> {
        let mut imports=Vec::with_capacity(v.imports.len());
        for name in v.imports{imports.push(self.text(&name)?);}
        let row=wire::ModuleNode{name:self.text(&v.name)?,imports:self.texts(&imports)?,records:self.refs(&v.records)?,
            type_sets:self.refs(&v.type_sets)?,functions:self.refs(&v.functions)?};
        Ok(wire::ModuleRef{offset:self.put_module(&row)?})
    }
    pub fn make_parallel(&mut self,v:ParallelDomain)->Result<wire::ParallelRef> {
        let row=wire::ParallelNode{dataset:self.text(&v.dataset)?,count:match v.count{Some(v)=>wire::Optional::some(v),None=>wire::Optional::none()},
            selection:match v.selection{Some(v)=>wire::Optional::some(v),None=>wire::Optional::none()}};
        Ok(wire::ParallelRef{offset:self.put_parallel(&row)?})
    }
    pub fn make_arm(&mut self,v:MatchArm)->Result<wire::ArmRef> {
        let binding=match v.binding{Some(v)=>wire::Optional::some(self.text(&v)?),None=>wire::Optional::none()};
        let row=wire::ArmNode{variant:self.text(&v.variant)?,binding,body:self.refs(&v.body)?,span:span(v.span)};
        Ok(wire::ArmRef{offset:self.put_arm(&row)?})
    }
    pub fn make_stmt(&mut self,v:Stmt)->Result<wire::StmtRef> {
        use StmtKind as K;use wire::StmtKind as W;
        let kind=match v.kind{
            K::Declare(ty,name)=>W::Declare(wire::DeclareStmt{ty:self.make_type(ty)?,name:self.text(&name)?}),
            K::Let(annotation,name,value)=>W::Let(wire::LetStmt{annotation:self.optional_type(annotation)?,name:self.text(&name)?,value}),
            K::Assign(target,operation,value)=>W::Assign(wire::AssignStmt{target,operation:self.text(&operation)?,value}),
            K::Expr(v)=>W::Expr(v),
            K::If(condition,yes,no)=>W::If(wire::IfStmt{condition,yes:self.refs(&yes)?,no:self.refs(&no)?}),
            K::Match(value,arms)=>W::Match(wire::MatchStmt{value,arms:self.refs(&arms)?}),
            K::While(condition,body)=>W::While(wire::WhileStmt{condition,body:self.refs(&body)?}),
            K::For(name,start,end,body)=>W::For(wire::ForStmt{name:self.text(&name)?,start,end:match end{Some(v)=>wire::Optional::some(v),None=>wire::Optional::none()},body:self.refs(&body)?}),
            K::Parallel(name,domain,body)=>W::Parallel(wire::ParallelStmt{name:self.text(&name)?,domain:self.make_parallel(domain)?,body:self.refs(&body)?}),
            K::Return(value)=>W::Return(match value{Some(v)=>wire::Optional::some(v),None=>wire::Optional::none()}),
            K::Break=>W::Break,K::Continue=>W::Continue,K::Phase=>W::Phase,
            K::Seek(name,value)=>W::Seek(wire::SeekStmt{name:self.text(&name)?,value})
        };
        Ok(wire::StmtRef{offset:self.put_stmt(&wire::StmtNode{kind,span:span(v.span)})?})
    }
}
