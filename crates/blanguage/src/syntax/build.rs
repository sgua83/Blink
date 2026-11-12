//! The ordinary-text callback constructs byte expressions in the one B store.
//! Grammar and module declarations are produced by B.
use super::{wire,Builder,Result};
use crate::ast::Span;

pub(crate) struct TypeName{pub name:String,pub arguments:Vec<wire::ArgumentRef>}
impl TypeName{pub fn named(name:impl Into<String>)->Self{Self{name:name.into(),arguments:Vec::new()}}}
pub(crate) enum GenericArgument{Type(TypeName),Value(wire::ExprRef)}
pub(crate) struct Expr{pub kind:ExprKind,pub span:Span}
pub(crate) enum ExprKind{Number(String),Construct(TypeName,Vec<wire::ExprRef>)}
fn span(value:Span)->wire::Span{wire::Span{start:value.start as u64,end:value.end as u64}}
/// Private repr(C) rows containing exactly one u64 offset.
pub(crate) unsafe trait Reference:Copy{}
unsafe impl Reference for wire::ArgumentRef{}
unsafe impl Reference for wire::ExprRef{}
impl Builder{
    pub fn refs<T:Reference>(&mut self,values:&[T])->Result<wire::Slice>{
        let result=unsafe{wire::indices(self.owner.raw.as_ptr(),values.as_ptr().cast_mut().cast(),values.len()as u64)};
        super::checked(result.error)?;Ok(result.value)
    }
    pub fn make_type(&mut self,v:TypeName)->Result<wire::TypeRef>{
        let row=wire::TypeNode{name:self.text(&v.name)?,canonical:0,arguments:self.refs(&v.arguments)?,
            dataset:0,view:0,tile:0,count:wire::Optional::none(),contracts:wire::Optional::none()};
        Ok(wire::TypeRef{offset:self.put_type(&row)?})
    }
    pub fn make_argument(&mut self,v:GenericArgument)->Result<wire::ArgumentRef>{
        let row=match v{GenericArgument::Type(v)=>wire::ArgumentNode::Type(self.make_type(v)?),
            GenericArgument::Value(v)=>wire::ArgumentNode::Value(wire::ShapeRef{offset:self.put_shape(&wire::ShapeNode::Expression(v))?})};
        Ok(wire::ArgumentRef{offset:self.put_argument(&row)?})
    }
    fn expr_row(&mut self,v:Expr)->Result<wire::ExprNode>{
        let kind=match v.kind{ExprKind::Number(v)=>wire::ExprKind::Number(self.text(&v)?),
            ExprKind::Construct(ty,args)=>wire::ExprKind::Construct(wire::ConstructExpr{ty:self.make_type(ty)?,arguments:self.refs(&args)?})};
        Ok(wire::ExprNode{kind,span:span(v.span)})
    }
    pub fn make_expr(&mut self,v:Expr)->Result<wire::ExprRef>{
        let row=self.expr_row(v)?;Ok(wire::ExprRef{offset:self.put_expr(&row)?})
    }
    pub(crate) fn parser_expr_row(&mut self,value:Expr)->Result<wire::ExprNode>{self.expr_row(value)}
}
