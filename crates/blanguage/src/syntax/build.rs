//! Remaining Rust module and ordinary-text constructors publish into the one B store.
//! Grammar-owned types, contracts and expressions are already stored by B.
use super::{wire,Builder,Result};
use crate::ast::{Access,FunctionKind,Span,Target};

pub(crate) struct TypeName {pub name:String,pub arguments:Vec<wire::ArgumentRef>}
impl TypeName {pub fn named(name:impl Into<String>)->Self {Self{name:name.into(),arguments:Vec::new()}}}
pub(crate) enum GenericArgument {Type(TypeName),Value(wire::ExprRef)}
pub(crate) struct Param {pub name:String,pub ty:wire::TypeRef,pub access:Access,pub span:Span}
pub(crate) struct Field {pub name:String,pub ty:wire::TypeRef,pub span:Span}
pub(crate) struct VariantDeclaration {pub name:String,pub payload:wire::TypeRef,pub discriminant:wire::ExprRef,pub span:Span}
pub(crate) struct Record {
    pub name:String,pub types:wire::Slice,pub values:wire::Slice,
    pub stride:Option<wire::ShapeRef>,pub enumeration:Option<wire::TypeRef>,pub variants:Vec<wire::VariantRef>,
    pub fields:Vec<wire::FieldRef>,pub span:Span
}
pub(crate) struct TypeSet {pub name:String,pub members:Vec<wire::TypeRef>,pub span:Span}
pub(crate) struct Function {
    pub name:String,pub types:wire::Slice,pub values:wire::Slice,
    pub params:Vec<wire::ParamRef>,pub result:wire::TypeRef,pub kind:FunctionKind,pub target:Target,
    pub contracts:Vec<wire::ContractRef>,pub body:Option<wire::Slice>,pub c_abi:bool,
    pub external:Option<crate::ast::ExternalName>,pub signature_end:usize,pub span:Span
}
pub(crate) struct Module {
    pub name:wire::Text,pub imports:Vec<wire::Text>,pub records:Vec<wire::RecordRef>,
    pub type_sets:Vec<wire::TypeSetRef>,pub functions:Vec<wire::FunctionRef>
}
pub(crate) struct Expr {pub kind:ExprKind,pub span:Span}
pub(crate) enum ExprKind {
    Number(String),Call(String,Vec<wire::ExprRef>),Construct(TypeName,Vec<wire::ExprRef>)
}
// The remaining Rust module parser synthesizes constant-function returns.
pub(crate) struct ReturnStmt {pub value:wire::ExprRef,pub span:Span}
fn span(value:Span)->wire::Span {wire::Span{start:value.start as u64,end:value.end as u64}}
/// All implementations are private repr(C) rows containing exactly one u64 offset.
pub(crate) unsafe trait Reference:Copy {}
unsafe impl Reference for wire::TypeRef {}
unsafe impl Reference for wire::ArgumentRef {}
unsafe impl Reference for wire::ContractRef {}
unsafe impl Reference for wire::FieldRef {}
unsafe impl Reference for wire::VariantRef {}
unsafe impl Reference for wire::RecordRef {}
unsafe impl Reference for wire::ParamRef {}
unsafe impl Reference for wire::FunctionRef {}
unsafe impl Reference for wire::TypeSetRef {}
unsafe impl Reference for wire::ExprRef {}
unsafe impl Reference for wire::StmtRef {}

impl Builder {
    pub fn refs<T:Reference>(&mut self,values:&[T])->Result<wire::Slice> {
        let result=unsafe{wire::indices(self.owner.raw.as_ptr(),values.as_ptr().cast_mut().cast(),values.len()as u64)};
        super::checked(result.error)?;Ok(result.value)
    }
    fn optional_type(&mut self,value:Option<wire::TypeRef>)->Result<wire::Optional<wire::TypeRef>> {
        Ok(match value{Some(v)=>wire::Optional::some(v),None=>wire::Optional::none()})
    }
    pub fn make_type(&mut self,v:TypeName)->Result<wire::TypeRef> {
        let row=wire::TypeNode{name:self.text(&v.name)?,canonical:0,
            arguments:self.refs(&v.arguments)?,dataset:0,view:0,tile:0,
            count:wire::Optional::none(),contracts:wire::Optional::none()};
        Ok(wire::TypeRef{offset:self.put_type(&row)?})
    }
    pub fn make_argument(&mut self,v:GenericArgument)->Result<wire::ArgumentRef> {
        let row=match v{GenericArgument::Type(v)=>wire::ArgumentNode::Type(self.make_type(v)?),
            GenericArgument::Value(v)=>wire::ArgumentNode::Value(wire::ShapeRef{offset:self.put_shape(&wire::ShapeNode::Expression(v))?})};
        Ok(wire::ArgumentRef{offset:self.put_argument(&row)?})
    }
    fn expr_row(&mut self,v:Expr)->Result<wire::ExprNode> {
        use ExprKind as K;use wire::ExprKind as W;
        let kind=match v.kind {
            K::Number(v)=>W::Number(self.text(&v)?),
            K::Call(name,args)=>W::Call(wire::CallExpr{name:self.text(&name)?,arguments:self.refs(&args)?}),
            K::Construct(ty,args)=>W::Construct(wire::ConstructExpr{ty:self.make_type(ty)?,arguments:self.refs(&args)?})
        };
        Ok(wire::ExprNode{kind,span:span(v.span)})
    }
    pub fn make_expr(&mut self,v:Expr)->Result<wire::ExprRef> {
        let row=self.expr_row(v)?;Ok(wire::ExprRef{offset:self.put_expr(&row)?})
    }
    pub fn make_param(&mut self,v:Param)->Result<wire::ParamRef> {
        let row=wire::ParamNode{name:self.text(&v.name)?,ty:v.ty,access:v.access,span:span(v.span)};
        Ok(wire::ParamRef{offset:self.put_param(&row)?})
    }
    pub fn make_field(&mut self,v:Field)->Result<wire::FieldRef> {
        let row=wire::FieldNode{name:self.text(&v.name)?,ty:v.ty,span:span(v.span)};
        Ok(wire::FieldRef{offset:self.put_field(&row)?})
    }
    pub fn make_variant(&mut self,v:VariantDeclaration)->Result<wire::VariantRef> {
        let row=wire::VariantNode{name:self.text(&v.name)?,payload:v.payload,discriminant:v.discriminant,span:span(v.span)};
        Ok(wire::VariantRef{offset:self.put_variant(&row)?})
    }
    pub fn make_record(&mut self,v:Record)->Result<wire::RecordRef> {
        let row=wire::RecordNode{name:self.text(&v.name)?,types:v.types,values:v.values,
            stride:match v.stride{Some(value)=>wire::Optional::some(value),None=>wire::Optional::none()},enumeration:self.optional_type(v.enumeration)?,
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
        let row=wire::FunctionNode{name:self.text(&v.name)?,types:v.types,values:v.values,params:self.refs(&v.params)?,
            result:v.result,kind:v.kind,target:v.target,contracts:self.refs(&v.contracts)?,
            body:match v.body{Some(value)=>wire::Optional::some(value),None=>wire::Optional::none()},c_abi:u32::from(v.c_abi),external,signature_end:v.signature_end as u64,span:span(v.span)};
        Ok(wire::FunctionRef{offset:self.put_function(&row)?})
    }
    pub fn make_module(&mut self,v:Module)->Result<wire::ModuleRef> {
        let row=wire::ModuleNode{name:v.name,imports:self.texts(&v.imports)?,records:self.refs(&v.records)?,
            type_sets:self.refs(&v.type_sets)?,functions:self.refs(&v.functions)?};
        Ok(wire::ModuleRef{offset:self.put_module(&row)?})
    }
    pub fn make_return(&mut self,v:ReturnStmt)->Result<wire::StmtRef> {
        let row=wire::StmtNode{kind:wire::StmtKind::Return(wire::Optional::some(v.value)),span:span(v.span)};
        Ok(wire::StmtRef{offset:self.put_stmt(&row)?})
    }
}

impl Builder {pub(crate) fn parser_expr_row(&mut self,value:Expr)->Result<wire::ExprNode>{self.expr_row(value)}}
