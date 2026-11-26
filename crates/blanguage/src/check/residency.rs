//! 资源位置属于函数的静态访问合同；逻辑输入路径不成为运行期状态。
use super::*;
use returns::Returned;
use std::{borrow::Cow, ffi::c_void, ptr, rc::Rc, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Part {
    Field(usize),
    Elements,
    Selected(Option<bounds::Selector>),
}

impl Part {
    fn from_binding(index: bounds::Selector, bindings: &BTreeMap<ValueIdentity, bounds::Proof>) -> Self {
        if let bounds::Selector::Direct(bounds::Operand::Value(id)) = index
            && let Some(proof) = bindings.get(&id)
        {
            if let Some((value, _)) = proof.known_integer_component(Scalar::U32, 0) {
                return Self::Selected(Some(bounds::Selector::Direct(bounds::Operand::Constant(Scalar::U32, value))));
            }
            if matches!(id, ValueIdentity::ElementIndex(_))
                && let Some(index) = proof.domains(0).iter().find_map(|domain| match *domain {
                    IndexDomain::Computed(expression) => expression.selector(),
                    _ => None,
                })
            {
                return Self::Selected(Some(index));
            }
        }
        Self::Selected(index.at_call(bindings))
    }

    fn at_call(&self, bindings: &BTreeMap<ValueIdentity, bounds::Proof>) -> Self {
        match self {
            Self::Selected(Some(index)) => Self::from_binding(*index, bindings),
            other => other.clone(),
        }
    }

    fn output_bound(&self, bindings: &BTreeMap<ValueIdentity, ValueIdentity>) -> Self {
        match self {
            Self::Selected(index) => Self::Selected(index.and_then(|index|
                index.map(|id| Some(bindings.get(&id).copied().unwrap_or(id))))),
            other => other.clone(),
        }
    }

    fn element_indices(&self, indices: &BTreeMap<ValueIdentity, bounds::Proof>) -> Self {
        match self {
            Self::Selected(Some(index)) if index.identity().is_some_and(|id| indices.contains_key(&id)) =>
                Self::from_binding(*index, indices),
            other => other.clone(),
        }
    }
}

fn path_family(fields: &[Part]) -> Vec<Part> {
    fields.iter().map(|part| match part {
        Part::Selected(_) => Part::Elements,
        other => other.clone(),
    }).collect()
}
fn path_model(fields: &[Part]) -> Vec<Part> {
    let mut depth = fields.iter().filter(|part| !matches!(part, Part::Field(_))).count();
    fields.iter().map(|part| match part {
        Part::Elements => {
            depth -= 1;
            Part::Selected(Some(bounds::Selector::Direct(bounds::Operand::Value(ValueIdentity::ElementIndex(depth)))))
        },
        Part::Selected(_) => { depth -= 1; part.clone() },
        other => other.clone(),
    }).collect()
}
fn path_selection(fields: &[Part]) -> BTreeMap<ValueIdentity, bounds::Proof> {
    let mut depth = fields.iter().filter(|part| !matches!(part, Part::Field(_))).count();
    fields.iter().filter_map(|part| match part {
        Part::Field(_) => None,
        part => {
            depth -= 1;
            let ordinal = ValueIdentity::ElementIndex(depth);
            let proof = match part {
                Part::Selected(Some(bounds::Selector::Direct(index))) =>
                    bounds::Proof::element_operand(*index, (0, u32::MAX)),
                Part::Selected(Some(index @ bounds::Selector::Affine { .. })) =>
                    bounds::Proof::default()
                        .with_domains(Domains::from([IndexDomain::Computed(bounds::IntegerExpression::selection(*index))])),
                _ => bounds::Proof::default(),
            };
            Some((ordinal, proof))
        },
    }).collect()
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Input {
    pub parameter: usize,
    pub fields: Vec<Part>,
}
impl Input {
    pub(super) fn depth(&self)->usize{self.fields.iter().filter(|part|!matches!(part,Part::Field(_))).count()}
    pub(super) fn family(&self)->Self{Self{parameter:self.parameter,fields:path_family(&self.fields)}}
    fn selection(&self)->BTreeMap<ValueIdentity,bounds::Proof>{path_selection(&self.fields)}
    pub(super) fn model(&self)->Self{Self{parameter:self.parameter,fields:path_model(&self.fields)}}
    fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{
        Self{parameter:self.parameter,fields:self.fields.iter().map(|part|part.at_call(bindings)).collect()}
    }
    fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{
        Self{parameter:self.parameter,fields:self.fields.iter().map(|part|part.output_bound(bindings)).collect()}
    }
    fn element_indices(&self,indices:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{Self{parameter:self.parameter,fields:self.fields.iter().map(|part|part.element_indices(indices)).collect()}}
    pub fn new(parameter: usize) -> Self {
        Self {
            parameter,
            fields: Vec::new(),
        }
    }
}
fn scoped_indices<'a>(
    bindings: impl IntoIterator<Item = (&'a ValueIdentity, &'a bounds::Proof)>,
    depth: usize,
) -> BTreeMap<ValueIdentity,bounds::Proof> {
    let mut out=bindings.into_iter().map(|(id,value)|(match id{ValueIdentity::ElementIndex(index)=>ValueIdentity::ElementIndex(index+depth),other=>*other},value.scope_element_indices(depth))).collect::<BTreeMap<_,_>>();
    for index in 0..depth{let id=ValueIdentity::ElementIndex(index);out.insert(id,bounds::Proof::default().with_identity(id));}out
}
fn scoped_names(bindings:&BTreeMap<ValueIdentity,ValueIdentity>,depth:usize)->BTreeMap<ValueIdentity,ValueIdentity>{
    let shift=|id|match id{ValueIdentity::ElementIndex(index)=>ValueIdentity::ElementIndex(index+depth),other=>other};
    bindings.iter().map(|(a,b)|(shift(*a),shift(*b))).collect()
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Origin {
    Cpu,
    Gpu,
    Input(Input),
    Output(Output),
    TaskTransfer,
}
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Residency {
    // Absent 是原来源方程的空解；Origins 的空集合仍表示实际未知位置。
    Absent,
    Origins(BTreeSet<Origin>),
    Selected(Box<bounds::Condition>,Box<Residency>,Box<Residency>),
}
impl Default for Residency{fn default()->Self{Self::Origins(BTreeSet::new())}}
impl Residency {
    pub fn cpu()->Self{Self::Origins(BTreeSet::from([Origin::Cpu]))}
    pub fn gpu()->Self{Self::Origins(BTreeSet::from([Origin::Gpu]))}
    // Successful task transfer has visited and moved every live owned CPU allocation.
    pub(super) fn task_transfer() -> Self { Self::Origins(BTreeSet::from([Origin::TaskTransfer])) }
    pub fn parameter(parameter:usize)->Self{Self::input(Input::new(parameter))}
    pub(super) fn input(input:Input)->Self{Self::Origins(BTreeSet::from([Origin::Input(input)]))}
    fn absent(&self)->bool{matches!(self,Self::Absent)}
    pub(super) fn selected(when: owned_presence::Presence, yes: Self, no: Self) -> Self {
        Self::conditional(when.condition(), yes, no)
    }
    pub(super) fn conditional(when: bounds::Condition, yes: Self, no: Self) -> Self {
        let plan = unsafe {
            choose_residency(ptr::from_ref(&when).cast(), ptr::from_ref(&yes).cast(), ptr::from_ref(&no).cast(), residency_algebra_host())
        };
        match plan.action {
            ChoiceAction::Absent => Self::Absent,
            ChoiceAction::Yes => yes.descend(plan.yes_depth, true),
            ChoiceAction::No => no.descend(plan.no_depth, false),
            ChoiceAction::Join => {
                let mut result = yes;
                result.join(&no);
                result
            }
            ChoiceAction::Select => {
                let yes = yes.descend(plan.yes_depth, true);
                let no = no.descend(plan.no_depth, false);
                match (yes, no) {
                    (Self::Selected(first, yy, yn), no) if plan.split_yes != 0 => {
                        let (ny, nn) = no.fork(plan.split_no != 0);
                        Self::reorder(when, *first, *yy, *yn, ny, nn)
                    }
                    (yes, Self::Selected(first, ny, nn)) if plan.split_no != 0 => {
                        let (yy, yn) = yes.fork(false);
                        Self::reorder(when, *first, yy, yn, *ny, *nn)
                    }
                    (yes, no) => Self::Selected(Box::new(when), Box::new(yes), Box::new(no)),
                }
            }
        }
    }
    fn descend(mut self, depth: usize, yes: bool) -> Self {
        for _ in 0..depth {
            self = match self {
                Self::Selected(_, left, right) => if yes { *left } else { *right },
                value => value,
            };
        }
        self
    }
    fn fork(self, split: bool) -> (Self, Self) {
        match self {
            Self::Selected(_, yes, no) if split => (*yes, *no),
            value => (value.clone(), value),
        }
    }
    fn reorder(when: bounds::Condition, first: bounds::Condition, yy: Self, yn: Self, ny: Self, nn: Self) -> Self {
        let yes = Self::conditional(when.clone(), yy, ny);
        let no = Self::conditional(when, yn, nn);
        Self::conditional(first, yes, no)
    }
    pub fn join(&mut self, other: &Self) {
        let plan = unsafe {
            join_residency(ptr::from_ref(self).cast(), ptr::from_ref(other).cast(), residency_algebra_host())
        };
        match plan.action {
            JoinAction::Keep => {}
            JoinAction::Copy => *self = other.clone(),
            JoinAction::Clear => *self = Self::default(),
            JoinAction::Extend => {
                if let (Self::Origins(left), Self::Origins(right)) = (self, other) {
                    left.extend(right.iter().cloned());
                }
            }
            JoinAction::Select => {
                // Borrowed right arms remain owned by other. The left pivot, when chosen,
                // moves with its two arms into the replacement.
                let (first, mut yes, mut no) = match std::mem::take(self) {
                    Self::Selected(first, yes, no) if plan.split_left != 0 => (*first, *yes, *no),
                    value => (
                        unsafe { &*plan.condition.cast::<bounds::Condition>() }.clone(),
                        value.clone(),
                        value,
                    ),
                };
                yes.join(unsafe { &*plan.right_yes.cast::<Self>() });
                no.join(unsafe { &*plan.right_no.cast::<Self>() });
                *self = Self::conditional(first, yes, no);
            }
        }
    }
    pub(super) fn under(&self,path:&bounds::Clause)->Self{match self{
        Self::Selected(when,a,b)=>match when.truth_in(path){Some(true)=>a.under(path),Some(false)=>b.under(path),None=>Self::conditional(when.as_ref().clone(),a.under(&when.assume(path,true)),b.under(&when.assume(path,false)))},
        _=>self.clone(),
    }}
    // A container's location does not determine the location of an owned child.
    // The original input/output resource path carries this projection.
    pub(super) fn project(&self,fields:&[Part],selection:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{
        if fields.is_empty(){return self.clone();}
        match self{
            Self::Absent=>Self::Absent,
            Self::Selected(when,a,b)=>Self::conditional(when.as_ref().clone(),a.project(fields,selection),b.project(fields,selection)),
            Self::Origins(origins)=>{
                if origins.is_empty(){return Self::default();}
                let mut result=Self::Absent;
                for origin in origins{
                    let value=match origin{
                        Origin::Input(input)=>{let mut input=input.clone();input.fields.extend_from_slice(fields);Self::input(input)},
                        Origin::Output(output)=>{
                            let mut output=output.clone();let depth=fields.iter().filter(|part|!matches!(part,Part::Field(_))).count();
                            output.fields.extend(path_family(fields));
                            output.bindings=output.bindings.into_iter().map(|(id,proof)|(match id{ValueIdentity::ElementIndex(index)=>ValueIdentity::ElementIndex(index+depth),other=>other},proof)).collect();
                            output.bindings.extend(selection.iter().filter(|(id,_)|matches!(id,ValueIdentity::ElementIndex(index)if *index<depth)).map(|(id,proof)|(*id,proof.clone())));
                            Self::Origins(BTreeSet::from([Origin::Output(output)]))
                        },
                        Origin::Cpu|Origin::Gpu=>Self::default(),
                        Origin::TaskTransfer=>Self::task_transfer(),
                    };result.join(&value);
                }
                result
            },
        }
    }
    pub(super) fn at_call(&self,bindings:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{match self{
        Self::Selected(when,a,b)=>Self::conditional(when.at_call(bindings),a.at_call(bindings),b.at_call(bindings)),
        Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{Origin::Input(input)=>Origin::Input(input.at_call(bindings)),Origin::Output(output)=>Origin::Output(Output{function:output.function.clone(),kind:output.kind.clone(),fields:output.fields.clone(),arguments:Arc::new(output.arguments.iter().map(|(input,value)|(input.clone(),value.at_call(&scoped_indices(bindings,input.depth())))).collect()),bindings:output.bindings.iter().map(|(input,value)|(*input,value.at_call(bindings))).collect()}),_=>origin.clone()}).collect()),
        Self::Absent=>Self::Absent,
    }}
    pub(super) fn with_aliases(&self,aliases:&bounds::Clause)->Self{match self{
        Self::Selected(when,a,b)=>{
            let yes=when.assume(aliases,true);let no=when.assume(aliases,false);
            Self::conditional(when.with_aliases(aliases),a.with_aliases(&yes),b.with_aliases(&no))
        },_=>self.clone(),
    }}
    pub(super) fn output_bound(&self,bindings:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{match self{
        Self::Selected(when,a,b)=>Self::conditional(when.output_bound(bindings),a.output_bound(bindings),b.output_bound(bindings)),
        Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{Origin::Input(input)=>Origin::Input(input.output_bound(bindings)),Origin::Output(output)=>Origin::Output(Output{function:output.function.clone(),kind:output.kind.clone(),fields:output.fields.clone(),arguments:Arc::new(output.arguments.iter().map(|(input,value)|(input.clone(),value.output_bound(&scoped_names(bindings,input.depth())))).collect()),bindings:output.bindings.iter().map(|(input,value)|(*input,value.output_bound(bindings))).collect()}),_=>origin.clone()}).collect()),
        Self::Absent=>Self::Absent,
    }}
    pub(super) fn element_indices(&self, indices: &BTreeMap<ValueIdentity, bounds::Proof>) -> Self {
        let unnamed = |proof: &bounds::Proof| {
            proof.known_identity(0).is_none()
                && matches!(proof.index_expression().and_then(bounds::IntegerExpression::selector), Some(bounds::Selector::Affine { .. }))
        };
        if !indices.values().any(unnamed) {
            return self.rebind_elements(indices);
        }
        let mut next = self.index_names().into_iter()
            .chain(indices.keys().copied())
            .chain(indices.values().flat_map(bounds::Proof::scalar_identities))
            .filter_map(|id| match id { ValueIdentity::ElementIndex(index) => Some(index + 1), _ => None })
            .max().unwrap_or(0);

        // The selection expression names the outer path; its result binds one inner ordinal.
        let mut bindings = indices.clone();
        for proof in bindings.values_mut().filter(|proof| unnamed(proof)) {
            *proof = std::mem::take(proof).with_identity(ValueIdentity::ElementIndex(next));
            next += 1;
        }
        self.rebind_elements(&bindings)
    }

    fn rebind_elements(&self, indices: &BTreeMap<ValueIdentity, bounds::Proof>) -> Self {
        if indices.is_empty(){return self.clone();}
        match self{
            Self::Absent=>Self::Absent,
            Self::Selected(when,a,b)=>Self::conditional(when.substitute_index_values(indices),a.rebind_elements(indices),b.rebind_elements(indices)),
            Self::Origins(origins)=>Self::Origins(origins.iter().map(|origin|match origin{
                Origin::Input(input)=>Origin::Input(input.element_indices(indices)),
                Origin::Output(output)=>{
                    Origin::Output(Output {
                        function: output.function.clone(), kind: output.kind.clone(), fields: output.fields.clone(),
                        arguments: Arc::new(output.arguments.iter().map(|(input,value)|(input.clone(),value.rebind_elements(&scoped_indices(indices,input.depth())))).collect()),
                        bindings: output.bindings.iter().map(|(id,proof)|(*id,proof.substitute_values(indices))).collect(),
                    })
                },
                other=>other.clone(),
            }).collect()),
        }
    }
    // 模型路径绑定自己的下标；嵌套实参的模型下标不属于外层作用域。
    fn index_names(&self)->BTreeSet<ValueIdentity>{
        let mut names=BTreeSet::new();
        match self{
            Self::Selected(when,a,b)=>{names.extend(when.scalar_identities());names.extend(a.index_names());names.extend(b.index_names());},
            Self::Origins(origins)=>for origin in origins{match origin{
                Origin::Input(input)=>names.extend(input.fields.iter().filter_map(|part| match part {
                    Part::Selected(Some(index)) => index.identity(),
                    _ => None,
                })),
                Origin::Output(output)=>{
                    for proof in output.bindings.values(){names.extend(proof.scalar_identities());}
                    for(input,value)in output.arguments.iter(){names.extend(value.index_names().into_iter().filter_map(|id|match id{ValueIdentity::ElementIndex(index)if index>=input.depth()=>Some(ValueIdentity::ElementIndex(index-input.depth())),_=>None}));}
                },
                _=>{},
            }},
            Self::Absent=>{},
        }
        names.retain(|id|matches!(id,ValueIdentity::ElementIndex(_)));names
    }
    pub(super) fn scope_indices(&self,depth:usize,bound:usize)->Self{
        let names=self.index_names().into_iter().filter_map(|id|match id{ValueIdentity::ElementIndex(index)if index>=bound=>Some((id,ValueIdentity::ElementIndex(index+depth))),_=>None}).collect();
        self.output_bound(&names)
    }
    fn bind_into(&self, arguments: &Arguments, result: &mut Self, materialize: bool) -> bool {
        let visit = ResidencyBinding {
            arguments: ptr::from_ref(arguments).cast(),
            result: ptr::from_mut(result).cast(),
            materialize: u32::from(materialize),
        };
        unsafe { bind_residency(visit, ptr::from_ref(self).cast(), RESIDENCY_BINDING_HOST) != 0 }
    }

    pub(super) fn bind(&self, arguments: &Arguments) -> Option<Self> {
        let mut result = Self::Absent;
        self.bind_into(arguments, &mut result, true).then_some(result)
    }
}
pub(super) type Requirements=BTreeMap<(Input,Target),BTreeSet<bounds::Condition>>;

#[repr(C)]
struct ResidencyBinding {
    arguments: Borrowed,
    result: Context,
    materialize: u32,
}

#[repr(C)]
struct ResidencyBindingHost {
    inspect: unsafe extern "C" fn(Borrowed) -> ResidencyCheckNode,
    choice: unsafe extern "C" fn(ResidencyBinding, ResidencyBranch) -> u32,
    origins: unsafe extern "C" fn(ResidencyBinding, Borrowed) -> u32,
}

#[repr(C)]
struct ResidencyBindingOriginsHost {
    next: unsafe extern "C" fn(Context) -> ResidencyOriginItem,
    bound: unsafe extern "C" fn(Context) -> u32,
    unknown: unsafe extern "C" fn(Context),
    input: unsafe extern "C" fn(ResidencyBinding, Borrowed) -> u32,
    output: unsafe extern "C" fn(ResidencyBinding, Borrowed),
    concrete: unsafe extern "C" fn(Context, Borrowed),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::bind_residency"]
    fn bind_residency(visit: ResidencyBinding, value: Borrowed, host: ResidencyBindingHost) -> u32;
    #[link_name = "bootstrap.ownership_path::bind_residency_origins"]
    fn bind_residency_origins(visit: ResidencyBinding, iterator: Context, count: usize, host: ResidencyBindingOriginsHost) -> u32;
}

const RESIDENCY_BINDING_HOST: ResidencyBindingHost = ResidencyBindingHost {
    inspect: inspect_residency_visit,
    choice: bound_choice,
    origins: bound_origins,
};

unsafe extern "C" fn bound_choice(visit: ResidencyBinding, branch: ResidencyBranch) -> u32 {
    let arguments = unsafe { &*visit.arguments.cast::<Arguments>() };
    let when = unsafe { &*branch.condition.cast::<bounds::Condition>() };
    let yes = unsafe { &*branch.yes.cast::<Residency>() }.bind(arguments);
    let no = unsafe { &*branch.no.cast::<Residency>() }.bind(arguments);
    let value = Residency::conditional(
        when.clone(), yes.unwrap_or(Residency::Absent), no.unwrap_or(Residency::Absent),
    );
    let bound = u32::from(!value.absent());
    if visit.materialize != 0 {
        unsafe { *visit.result.cast::<Residency>() = value };
    }
    bound
}

unsafe extern "C" fn bound_origins(visit: ResidencyBinding, origins: Borrowed) -> u32 {
    let origins = unsafe { &*origins.cast::<BTreeSet<Origin>>() };
    let mut iterator = origins.iter();
    let host = ResidencyBindingOriginsHost {
        next: visit_next_origin, bound: bound_result, unknown: set_bound_unknown,
        input: bind_input_origin, output: bind_output_origin, concrete: bind_concrete_origin,
    };
    unsafe { bind_residency_origins(visit, ptr::from_mut(&mut iterator).cast(), origins.len(), host) }
}

unsafe extern "C" fn bound_result(result: Context) -> u32 {
    let result = unsafe { &*result.cast::<Residency>() };
    u32::from(!result.absent())
}

unsafe extern "C" fn set_bound_unknown(result: Context) {
    unsafe { *result.cast::<Residency>() = Residency::default() };
}

fn merge_bound_source(result: &mut Residency, value: Residency) {
    if result.absent() {
        *result = value;
    } else {
        result.join(&value);
    }
}

unsafe extern "C" fn bind_input_origin(visit: ResidencyBinding, input: Borrowed) -> u32 {
    let arguments = unsafe { &*visit.arguments.cast::<Arguments>() };
    let input = unsafe { &*input.cast::<Input>() };
    let Some(value) = argument_value(arguments, input, || input.selection()) else { return 0; };
    let present = u32::from(!value.absent());
    merge_bound_source(unsafe { &mut *visit.result.cast::<Residency>() }, value);
    present
}

unsafe extern "C" fn bind_output_origin(visit: ResidencyBinding, output: Borrowed) {
    let arguments = unsafe { &*visit.arguments.cast::<Arguments>() };
    let output = unsafe { &*output.cast::<Output>() };
    let projected = Output {
        function: output.function.clone(),
        kind: output.kind.clone(),
        fields: output.fields.clone(),
        arguments: Arc::new(output.arguments.iter().filter_map(|(input, value)| {
            value.bind(arguments).map(|value| (input.clone(), value))
        }).collect()),
        bindings: output.bindings.clone(),
    };
    let value = Residency::Origins(BTreeSet::from([Origin::Output(projected)]));
    merge_bound_source(unsafe { &mut *visit.result.cast::<Residency>() }, value);
}

unsafe extern "C" fn bind_concrete_origin(result: Context, origin: Borrowed) {
    let origin = unsafe { &*origin.cast::<Origin>() };
    let value = Residency::Origins(BTreeSet::from([origin.clone()]));
    merge_bound_source(unsafe { &mut *result.cast::<Residency>() }, value);
}

pub(super) type Arguments=BTreeMap<Input,Residency>;
pub(super) type SharedArguments = Arc<Arguments>;
struct ArgumentLookup<'a> {
    arguments: &'a Arguments,
    key: Input,
}

unsafe extern "C" fn lookup_argument_path(context: Context, fields: usize) -> Borrowed {
    let lookup = unsafe { &mut *context.cast::<ArgumentLookup<'_>>() };
    lookup.key.fields.truncate(fields);
    lookup.arguments.get(&lookup.key).map_or(ptr::null(), |value| ptr::from_ref(value).cast())
}

fn argument_value(
    arguments: &Arguments,
    input: &Input,
    selection: impl FnOnce() -> BTreeMap<ValueIdentity, bounds::Proof>,
) -> Option<Residency> {
    let mut lookup = ArgumentLookup { arguments, key: input.family() };
    let fields = lookup.key.fields.len();
    let prefix = unsafe {
        find_residency_prefix(ptr::from_mut(&mut lookup).cast(), fields, lookup_argument_path)
    };
    let value = unsafe { prefix.residency.cast::<Residency>().as_ref() }?;
    if value.absent() {
        return Some(Residency::Absent);
    }

    let selection = selection();
    let suffix = &input.fields[prefix.fields..];
    let removed = suffix.iter().filter(|part| !matches!(part, Part::Field(_))).count();
    let value = if removed == 0 {
        value.element_indices(&selection)
    } else {
        let prefix_selection = selection.iter().filter_map(|(id, proof)| match id {
            ValueIdentity::ElementIndex(index) if *index >= removed =>
                Some((ValueIdentity::ElementIndex(index - removed), proof.clone())),
            _ => None,
        }).collect();
        value.element_indices(&prefix_selection)
    };
    if suffix.is_empty() {
        return Some(value);
    }

    // Model ordinals count from the end of the path. Only the selected suffix is needed here.
    let mut selected = path_model(&path_family(suffix));
    for part in &mut selected {
        *part = part.element_indices(&selection);
    }
    Some(value.project(&selected, &selection))
}

#[derive(Clone)]
pub(super) struct Call{
    pub function:String,
    pub arguments:SharedArguments,
    pub bindings:BTreeMap<ValueIdentity,bounds::Proof>,
    pub path:bounds::Clause,
    pub source:Option<Source>,
    pub span:Span,
}
// 每个定义只交接自己的原使用与调用；递归关系不展开成后代路径清单。
#[derive(Clone,Default)]
pub(super) struct Contract {
    pub required:Requirements,
    pub calls:Vec<Call>,
}
impl Contract {
    pub(super) fn normalize_contract(&mut self,normal:&mut bounds::Normalizer) {
        for conditions in self.required.values_mut() {
            let mut result=normal.condition(&bounds::Condition::constant(false));
            for condition in std::mem::take(conditions){result=normal.union(&result,&condition);}
            conditions.insert(result);
        }
        for call in &mut self.calls {
            for value in Arc::make_mut(&mut call.arguments).values_mut(){value.normalize_contract(normal);}
            for value in call.bindings.values_mut(){normal.proof(value);}
            call.path=normal.clause(&call.path);
        }
    }
}
impl Residency {
    pub(super) fn normalize_contract(&mut self,normal:&mut bounds::Normalizer) {
        match self {
            Self::Absent=>{},
            Self::Selected(when,yes,no)=>{**when=normal.condition(when);yes.normalize_contract(normal);no.normalize_contract(normal);},
            Self::Origins(origins)=>{
                let previous=std::mem::take(origins);
                for mut origin in previous {
                    if let Origin::Output(output)=&mut origin {
                        for value in Arc::make_mut(&mut output.arguments).values_mut(){value.normalize_contract(normal);}
                        for value in output.bindings.values_mut(){normal.proof(value);}
                    }
                    origins.insert(origin);
                }
            },
        }
    }
}




#[derive(Default)]
pub(super) struct Inputs{
    pub calls:Vec<Call>,
    pub uses:Vec<Use>,
}
// 原使用点是局部检查与公共要求的共同来源，不维护另一份无条件输入状态。
impl Inputs{
    pub(super) fn known_at(&self,value:&Residency,target:Target,path:&bounds::Clause)->bool {
        let location=match target{Target::Cpu=>Residency::cpu(),Target::Gpu=>Residency::gpu(),Target::Compute=>return false};
        equal_under(value,&location,path,&mut|_,_,_|false)||self.uses.iter().any(|usage|matches!(usage,
            Use::At{value:used,target:actual,path:when,..}if *actual==target
                &&bounds::Condition::guard(when.clone()).truth_in(path)==Some(true)
                &&equal_under(value,used,path,&mut|_,_,_|false)))
    }

    pub fn require(&mut self,value:&Residency,target:Target,source:&Source,span:Span,path:&bounds::Clause)->bool{
        if target==Target::Compute||!known(value,target,path){return false;}
        self.uses.push(Use::At{value:value.clone(),target,path:path.clone(),source:source.clone(),span});true
    }
    pub fn same(&mut self,a:&Residency,b:&Residency,source:&Source,span:Span,path:&bounds::Clause){
        if a != b {
            self.uses.push(Use::Same{before:a.clone(),after:b.clone(),path:path.clone(),source:source.clone(),span});
        }
    }
}
fn known(value: &Residency, target: Target, path: &bounds::Clause) -> bool {
    let visit = ResidencyVisit::new(value, target, path, ResidencyCheckMode::Known);
    unsafe { visit.run(ptr::null_mut()) }.accepted != 0
}
fn insert(required:&mut Requirements,input:Input,target:Target,when:bounds::Condition)->bool{
    if !when.active(){return true;}
    if required.iter().any(|((other,place),guards)|*other==input&&*place!=target&&guards.iter().any(|guard|guard.assume(&when.assume(&bounds::Clause::default(),true),true).overlaps(&bounds::Clause::default()))){return false;}
    required.entry((input,target)).or_default().insert(when);true
}

fn consume(required: &mut Requirements, value: &Residency, target: Target, path: &bounds::Clause) -> bool {
    let visit = ResidencyVisit::new(value, target, path, ResidencyCheckMode::Require);
    unsafe { visit.run(ptr::from_mut(required).cast()) }.accepted != 0
}
pub(super) fn input_parameters(parameters:&BTreeMap<ValueIdentity,ValueIdentity>,width:usize)->BTreeMap<ValueIdentity,bounds::Proof>{
    parameters.iter().filter(|(_,target)|match target{ValueIdentity::Register(n)=>*n<width,ValueIdentity::ParameterContent(..)|ValueIdentity::ResourceParameter(..)=>true,_=>false}).map(|(source,target)|(*source,bounds::Proof::default().with_identity(*target))).collect()
}
pub(super) fn canonical(required:&Requirements,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,source:&Source,span:Span)->Result<Requirements>{
    let mut result=Requirements::new();
    for((input,target),guards)in required{let bindings=scoped_indices(parameters,input.depth());for guard in guards{
        if !insert(&mut result,input.at_call(&bindings),*target,guard.guard_at_call(&bindings)){return source.fail(span,"B266","输入资源在同一接口条件下同时要求CPU和GPU存储");}
    }}Ok(result)
}
type LoopRequirements = BTreeMap<Input, BTreeMap<Target, BTreeSet<bounds::Condition>>>;

fn local_loop_requirements(
    required: Requirements,
    parameters: &BTreeMap<ValueIdentity, bounds::Proof>,
) -> LoopRequirements {
    let mut inverse = BTreeMap::new();
    for (source, target) in parameters {
        let target = target.known_identity(0).unwrap();
        let entry = inverse.entry(target).or_insert_with(|| bounds::Proof::default().with_identity(*source));
        if !matches!(target, ValueIdentity::Register(_)) && *source == target {
            *entry = bounds::Proof::default().with_identity(*source);
        }
    }
    let mut local = LoopRequirements::new();
    for ((input, target), guards) in required {
        let bindings = scoped_indices(&inverse, input.depth());
        local.entry(input.at_call(&bindings)).or_default().entry(target).or_default()
            .extend(guards.into_iter().map(|guard| guard.guard_at_call(&bindings)));
    }
    local
}

pub(super) fn infer(inputs:&Inputs,parameters:&BTreeMap<ValueIdentity,bounds::Proof>,outputs:&Outputs,source:&Source,span:Span)->Result<Contract>{
    let mut required=Requirements::new();
    for usage in &inputs.uses{match usage {
        Use::At{value,target,path,source,span}=>{
            let value=evaluate(value,outputs,path);
            if !value.absent()&&!consume(&mut required,&value,*target,path){return source.fail(*span,"B266","实际资源来源不满足本次条件CPU／GPU位置要求");}
        },
        Use::Same{before,after,path,source,span}=>{
            let before=evaluate(before,outputs,path);let after=evaluate(after,outputs,path);
            if !before.absent()&&!after.absent()&&!equal_under(&before,&after,path,&mut|a,b,path|{
                if let Some(target)=physical_target(a){consume(&mut required,b,target,path)}
                else if let Some(target)=physical_target(b){consume(&mut required,a,target,path)}
                else{true}
            }){return source.fail(*span,"B266","循环实际相等位置要求与资源存储位置不一致");}
        },
    }}
    let required=canonical(&required,parameters,source,span)?;
    let calls=inputs.calls.iter().map(|call|Call{
        function:call.function.clone(),
        arguments:Arc::new(call.arguments.iter().map(|(input,value)|(input.clone(),value.at_call(&scoped_indices(parameters,input.depth())))).collect()),
        bindings:call.bindings.iter().map(|(id,proof)|(*id,proof.at_call(parameters))).collect(),
        path:bounds::Condition::guard(call.path.clone()).projected_case(true, parameters),
        source:None,span:Span::default(),
    }).collect();
    Ok(Contract{required,calls})
}

// The active key groups equivalent sources at their original paths. It keeps
// the wanted input prefix and follows direct output links without changing storage.
struct QueryValueKey<'a> {
    wanted: Option<&'a Input>,
    outputs: &'a Outputs,
    terms: BTreeMap<BTreeSet<Vec<u8>>, bounds::Clause>,
}

#[repr(C)]
struct QueryValuePath {
    value: Borrowed,
    path: Borrowed,
}

#[repr(C)]
struct QueryValueKeyHost {
    inspect: unsafe extern "C" fn(Borrowed) -> ResidencyCheckNode,
    branch: unsafe extern "C" fn(Context, QueryValuePath, Borrowed, u32),
    collect: unsafe extern "C" fn(Context, Borrowed, Borrowed),
}

#[repr(u32)]
enum KeyOriginProjection { Concrete, Output, Input }

#[repr(C)]
struct QueryOriginKeyHost {
    next: unsafe extern "C" fn(Context) -> ResidencyOriginItem,
    matches: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    append: unsafe extern "C" fn(Context, KeyOriginProjection, Borrowed),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::collect_query_value_key"]
    fn collect_query_value_key(context: Context, visit: QueryValuePath, host: QueryValueKeyHost);
    #[link_name = "bootstrap.ownership_path::collect_query_origin_keys"]
    fn collect_query_origin_keys(context: Context, cursor: Context, wanted: Borrowed, host: QueryOriginKeyHost);
}

const QUERY_VALUE_KEY_HOST: QueryValueKeyHost = QueryValueKeyHost {
    inspect: inspect_residency_visit,
    branch: query_value_key_branch,
    collect: query_value_key_group,
};

unsafe extern "C" fn query_value_key_branch(context: Context, mut visit: QueryValuePath, condition: Borrowed, truth: u32) {
    let condition = unsafe { &*condition.cast::<bounds::Condition>() };
    let path = condition.assume(unsafe { &*visit.path.cast::<bounds::Clause>() }, truth != 0);
    visit.path = ptr::from_ref(&path).cast();
    unsafe { collect_query_value_key(context, visit, QUERY_VALUE_KEY_HOST) };
}

struct QueryOriginKey<'a> {
    outputs: &'a Outputs,
    encoded: BTreeSet<Vec<u8>>,
}

unsafe extern "C" fn query_value_key_group(context: Context, origins: Borrowed, path: Borrowed) {
    use std::collections::btree_map::Entry;
    let context = unsafe { &mut *context.cast::<QueryValueKey<'_>>() };
    let path = unsafe { &*path.cast::<bounds::Clause>() };
    let mut origins = unsafe { &*origins.cast::<BTreeSet<Origin>>() }.iter();
    let mut key = QueryOriginKey { outputs: context.outputs, encoded: BTreeSet::new() };
    let wanted = context.wanted.map_or(ptr::null(), |input| ptr::from_ref(input).cast());
    let host = QueryOriginKeyHost {
        next: visit_next_origin,
        matches: query_input_prefix,
        append: append_query_origin_key,
    };
    unsafe {
        collect_query_origin_keys(ptr::from_mut(&mut key).cast(), ptr::from_mut(&mut origins).cast(), wanted, host);
    }
    match context.terms.entry(key.encoded) {
        Entry::Occupied(mut group) => {
            *group.get_mut() = bounds::Clause::either(group.get(), path);
        }
        Entry::Vacant(group) => {
            group.insert(path.clone());
        }
    }
}

unsafe extern "C" fn query_input_prefix(wanted: Borrowed, input: Borrowed) -> u32 {
    let wanted = unsafe { &*wanted.cast::<Input>() };
    let input = unsafe { &*input.cast::<Input>() };
    u32::from(wanted.parameter == input.parameter && wanted.fields.starts_with(&input.fields))
}

unsafe extern "C" fn append_query_origin_key(context: Context, projection: KeyOriginProjection, value: Borrowed) {
    use crate::module::codec::{Encode, Writer};
    let context = unsafe { &mut *context.cast::<QueryOriginKey<'_>>() };
    let mut writer = Writer(Vec::new());
    match projection {
        KeyOriginProjection::Concrete => unsafe { &*value.cast::<Origin>() }.encode(&mut writer),
        KeyOriginProjection::Output => {
            let output = unsafe { &*value.cast::<Output>() };
            context.encoded.insert(canonical_output_key(output, context.outputs));
            return;
        }
        KeyOriginProjection::Input => {
            2u8.encode(&mut writer);
            unsafe { value.cast::<Input>().as_ref() }.encode(&mut writer);
        }
    }
    context.encoded.insert(writer.0);
}

fn query_value(value: &Residency, wanted: Option<&Input>, outputs: &Outputs, writer: &mut crate::module::codec::Writer) {
    use crate::module::codec::Encode;
    let mut key = QueryValueKey { wanted, outputs, terms: BTreeMap::new() };
    let path = bounds::Clause::default();
    let visit = QueryValuePath { value: ptr::from_ref(value).cast(), path: ptr::from_ref(&path).cast() };
    unsafe { collect_query_value_key(ptr::from_mut(&mut key).cast(), visit, QUERY_VALUE_KEY_HOST) };
    key.terms.len().encode(writer);
    for (origins, path) in key.terms {
        origins.encode(writer);
        bounds::Condition::guard(path).encode(writer);
    }
}
fn query_bindings(bindings: &BTreeMap<ValueIdentity, bounds::Proof>, w: &mut crate::module::codec::Writer) {
    use crate::module::codec::Encode;

    bindings.len().encode(w);
    for (id, proof) in bindings {
        id.encode(w);
        let identity = proof.known_identity(0).filter(|_| proof.origins.len() == 1);
        identity.encode(w);
        if identity.is_none() {
            proof.encode(w);
        }
    }
}

fn query_key(call:&Call,wanted:Option<&Input>,outputs:&Outputs)->Vec<u8>{
    use crate::module::codec::Encode;
    let mut w=crate::module::codec::Writer(Vec::new());call.function.encode(&mut w);
    query_bindings(&call.bindings, &mut w);
    call.arguments.len().encode(&mut w);
    for(input,value)in call.arguments.iter(){input.encode(&mut w);query_value(value,wanted,outputs,&mut w);}
    w.0
}
fn query_consume(
    required: &mut Requirements, deferred: &mut bool, wanted: Option<&Input>,
    value: &Residency, target: Target, path: &bounds::Clause,
) -> bool {
    let mut visit = ResidencyVisit::new(value, target, path, ResidencyCheckMode::Query);
    visit.wanted = wanted.map_or(ptr::null(), |input| ptr::from_ref(input).cast());
    visit.deferred = u32::from(*deferred);
    let result = unsafe { visit.run(ptr::from_mut(required).cast()) };
    *deferred = result.deferred != 0;
    result.accepted != 0
}
// 声明中的Input映射还引用实参已有的后代模型，不把父位置当作完整子树。
fn visit_inputs(value: &Residency, visit: &mut impl FnMut(&Input)) {
    match value {
        Residency::Selected(_, yes, no) => {
            visit_inputs(yes, visit);
            visit_inputs(no, visit);
        }
        Residency::Origins(origins) => {
            for origin in origins {
                if let Origin::Input(input) = origin {
                    visit(input);
                }
            }
        }
        Residency::Absent => {}
    }
}

pub(super) fn inputs(value: &Residency, out: &mut BTreeSet<Input>) {
    visit_inputs(value, &mut |input| { out.insert(input.clone()); });
}

fn model_families(value: &Residency, depth: usize) -> Residency {
    match value {
        Residency::Selected(when, yes, no) => Residency::conditional(
            when.as_ref().clone(), model_families(yes, depth), model_families(no, depth),
        ),
        Residency::Origins(origins) => Residency::Origins(origins.iter().map(|origin| {
            let Origin::Input(input) = origin else { return origin.clone(); };
            let mut left = input.fields.iter().filter(|part| matches!(part, Part::Elements)).count();
            let fields = input.fields.iter().map(|part| match part {
                Part::Elements => {
                    left -= 1;
                    Part::Selected(Some(bounds::Selector::Direct(bounds::Operand::Value(
                        ValueIdentity::ElementIndex(depth + left),
                    ))))
                }
                other => other.clone(),
            }).collect();
            Origin::Input(Input { parameter: input.parameter, fields })
        }).collect()),
        Residency::Absent => Residency::Absent,
    }
}

struct ArgumentBinding<'a, 'iter> {
    prefix: &'a Input,
    value: &'a Residency,
    declared: &'a Arguments,
    actual: &'a Arguments,
    outputs: &'a Outputs,
    result: &'a mut Arguments,
    descendants: &'iter mut dyn Iterator<Item = &'a [Part]>,
    child: Input,
    suffix: &'a [Part],
}

impl ArgumentBinding<'_, '_> {
    fn resolve(&self, value: &Residency) -> Residency {
        let Some(bound) = value.bind(self.actual) else { return Residency::Absent; };
        if present(&bound, self.outputs) {
            bound
        } else {
            Residency::Absent
        }
    }

    fn child_from_suffix(&mut self) {
        self.child.fields.truncate(self.prefix.fields.len());
        self.child.fields.extend_from_slice(self.suffix);
    }
}

#[repr(C)]
struct ArgumentDescendant {
    fields: u64,
    bound: u32,
}

#[repr(C)]
struct ArgumentBindingHost {
    bind_root: unsafe extern "C" fn(Context),
    next_descendant: unsafe extern "C" fn(Context) -> ArgumentDescendant,
    declared_at: unsafe extern "C" fn(Context, u64) -> u32,
    bind_child: unsafe extern "C" fn(Context),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::bind_argument"]
    fn bind_argument(context: Context, prefix: u64, host: ArgumentBindingHost);
}

unsafe extern "C" fn bind_root(context: Context) {
    let binding = unsafe { &mut *context.cast::<ArgumentBinding<'_, '_>>() };
    let value = binding.resolve(binding.value);
    binding.result.insert(binding.prefix.clone(), value);
}

unsafe extern "C" fn next_descendant(context: Context) -> ArgumentDescendant {
    let binding = unsafe { &mut *context.cast::<ArgumentBinding<'_, '_>>() };
    let Some(suffix) = binding.descendants.next() else {
        return ArgumentDescendant { fields: 0, bound: 0 };
    };
    binding.suffix = suffix;
    binding.child_from_suffix();
    ArgumentDescendant {
        fields: binding.child.fields.len() as u64,
        bound: u32::from(binding.result.contains_key(&binding.child)),
    }
}

unsafe extern "C" fn declared_at(context: Context, fields: u64) -> u32 {
    let binding = unsafe { &mut *context.cast::<ArgumentBinding<'_, '_>>() };
    binding.child.fields.truncate(fields as usize);
    u32::from(binding.declared.contains_key(&binding.child))
}

unsafe extern "C" fn bind_child(context: Context) {
    let binding = unsafe { &mut *context.cast::<ArgumentBinding<'_, '_>>() };
    binding.child_from_suffix();
    let prefix_depth = binding.prefix.depth();
    let model = binding.child.model();
    let depth = model.depth() - prefix_depth;
    let shift = (0..prefix_depth).map(|index| (
        ValueIdentity::ElementIndex(index),
        bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(index + depth)),
    )).collect();
    let value = model_families(&binding.value.element_indices(&shift), depth).project(
        &model.fields[binding.prefix.fields.len()..], &model.selection(),
    );
    let value = binding.resolve(&value);
    binding.result.insert(binding.child.clone(), value);
}

fn bind_arguments(declared: &Arguments, actual: &Arguments, bindings: &BTreeMap<ValueIdentity, bounds::Proof>, outputs: &Outputs) -> Arguments {
    use std::ops::Bound::{Excluded, Unbounded};
    let mut result = Arguments::new();
    for (prefix, source) in declared {
        let value = source.at_call(&scoped_indices(bindings, prefix.depth()));
        let mut families = BTreeSet::new();
        visit_inputs(&value, &mut |input| { families.insert(input.family()); });
        let mut descendants = families.into_iter().flat_map(|family| {
            actual.range((Excluded(&family), Unbounded)).map_while(move |(input, _)| {
                if input.parameter != family.parameter {
                    return None;
                }
                input.fields.strip_prefix(family.fields.as_slice())
            })
        });
        let mut binding = ArgumentBinding {
            prefix, value: &value, declared, actual, outputs, result: &mut result,
            descendants: &mut descendants, child: prefix.clone(), suffix: &[],
        };
        let host = ArgumentBindingHost { bind_root, next_descendant, declared_at, bind_child };
        // The host borrows the original maps and current projected source for this synchronous call.
        unsafe { bind_argument(ptr::from_mut(&mut binding).cast(), prefix.fields.len() as u64, host); }
    }
    result
}
struct QueryFrame<'a> {
    call: Cow<'a, Call>,
    key: Rc<Vec<u8>>,
    required: &'a Requirements,
    children: std::slice::Iter<'a, Call>,
}

impl<'a> QueryFrame<'a> {
    fn new(call: Cow<'a, Call>, key: Rc<Vec<u8>>, known: &'a super::native_function::ResidencyContracts<'a>) -> Self {
        let contract = known.get(&call.function);
        Self { call, key, required: &contract.required, children: contract.calls.iter() }
    }
}

struct ResidencyQuery<'a> {
    known: &'a super::native_function::ResidencyContracts<'a>,
    outputs: &'a Outputs,
    wanted: Option<&'a Input>,
    source: &'a Source,
    span: Span,
    current: QueryFrame<'a>,
    parents: Vec<QueryFrame<'a>>,
    active: BTreeSet<Rc<Vec<u8>>>,
    required: Requirements,
    deferred: bool,
    error: Option<Diagnostic>,
}

impl<'a> ResidencyQuery<'a> {
    fn consume(&mut self) -> Result<()> {
        let call = &self.current.call;
        for ((input, target), guards) in self.current.required {
            if guards.is_empty() {
                continue;
            }
            let bindings = scoped_indices(&call.bindings, input.depth());
            let Some(value) = argument_value(&call.arguments, input, || {
                input.selection().into_iter().map(|(index, proof)| {
                    (index, proof.at_call(&bindings))
                }).collect()
            }) else { continue; };
            for guard in guards {
                let path = bounds::Clause::both(&call.path, &guard.projected_case(true, &bindings));
                let value = evaluate(&value, self.outputs, &path);
                if !query_consume(&mut self.required, &mut self.deferred, self.wanted, &value, *target, &path) {
                    let place = if *target == Target::Cpu { "CPU" } else { "GPU" };
                    return self.source.fail(self.span, "B266", format!(
                        "函数 {} 的第 {} 个资源参数（资源路径 {:?}）不满足实际生效条件下的{}存储要求",
                        call.function, input.parameter + 1, input.fields, place,
                    ));
                }
            }
        }
        Ok(())
    }

}

#[repr(C)]
struct ResidencyQueryHost {
    consume: unsafe extern "C" fn(Context) -> u32,
    next_child: unsafe extern "C" fn(Context) -> Borrowed,
    leave: unsafe extern "C" fn(Context) -> u32,
    project: unsafe extern "C" fn(Context, Borrowed) -> u32,
}

#[repr(C)]
struct ResidencyQueryChildHost {
    possible: unsafe extern "C" fn(Borrowed) -> u32,
    enter: unsafe extern "C" fn(Context, Borrowed, Context) -> u32,
    release: unsafe extern "C" fn(Context),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::traverse_residency_query"]
    fn traverse_residency_query(context: Context, host: ResidencyQueryHost);
    #[link_name = "bootstrap.ownership_path::enter_residency_query_child"]
    fn enter_residency_query_child(context: Context, child: Borrowed, path: Context, host: ResidencyQueryChildHost) -> u32;
}

unsafe extern "C" fn consume_query_frame(context: Context) -> u32 {
    let query = unsafe { &mut *context.cast::<ResidencyQuery<'_>>() };
    query.error = query.consume().err();
    u32::from(query.error.is_none())
}

unsafe extern "C" fn next_query_child(context: Context) -> Borrowed {
    let query = unsafe { &mut *context.cast::<ResidencyQuery<'_>>() };
    query.current.children.next().map_or(ptr::null(), |child| ptr::from_ref(child).cast())
}

unsafe extern "C" fn leave_query_frame(context: Context) -> u32 {
    let query = unsafe { &mut *context.cast::<ResidencyQuery<'_>>() };
    query.active.remove(&query.current.key);
    let Some(parent) = query.parents.pop() else { return 0; };
    query.current = parent;
    1
}

unsafe extern "C" fn project_query_child(context: Context, child: Borrowed) -> u32 {
    let path = {
        let query = unsafe { &*context.cast::<ResidencyQuery<'_>>() };
        let child = unsafe { &*child.cast::<Call>() };
        let call = &query.current.call;
        let condition = bounds::Condition::guard(child.path.clone());
        bounds::Clause::both(&call.path, &condition.projected_case(true, &call.bindings))
    };
    let mut path = std::mem::ManuallyDrop::new(path);
    let host = ResidencyQueryChildHost {
        possible: compare_possible, enter: store_query_child, release: drop_query_child_path,
    };
    // B either transfers this path to the attempted child or releases it.
    unsafe { enter_residency_query_child(context, child, ptr::from_mut(&mut *path).cast(), host) }
}

unsafe extern "C" fn drop_query_child_path(path: Context) {
    unsafe { path.cast::<bounds::Clause>().drop_in_place() };
}

unsafe extern "C" fn store_query_child(context: Context, child: Borrowed, path: Context) -> u32 {
    let query = unsafe { &mut *context.cast::<ResidencyQuery<'_>>() };
    let declared = unsafe { &*child.cast::<Call>() };
    let parent = &query.current.call;
    let arguments = bind_arguments(&declared.arguments, &parent.arguments, &parent.bindings, query.outputs);
    let bindings = declared.bindings.iter().map(|(id, proof)| (*id, proof.at_call(&parent.bindings))).collect();
    let path = unsafe { path.cast::<bounds::Clause>().read() };
    let child = Call {
        function: declared.function.clone(), arguments: Arc::new(arguments), bindings, path, source: None, span: Span::default(),
    };
    let key = Rc::new(query_key(&child, query.wanted, query.outputs));
    if !query.active.insert(Rc::clone(&key)) {
        return 0;
    }
    let frame = QueryFrame::new(Cow::Owned(child), key, query.known);
    query.parents.push(std::mem::replace(&mut query.current, frame));
    1
}

fn query(call: &Call, known: &super::native_function::ResidencyContracts<'_>, outputs: &Outputs, wanted: Option<&Input>,
    source: &Source, span: Span, deferred: bool) -> Result<(Requirements, bool)> {
    let required = Requirements::new();
    if !call.path.possible() {
        return Ok((required, deferred));
    }
    let key = Rc::new(query_key(call, wanted, outputs));
    let current = QueryFrame::new(Cow::Borrowed(call), Rc::clone(&key), known);
    let mut query = ResidencyQuery {
        known, outputs, wanted, source, span, current, parents: Vec::new(),
        active: BTreeSet::from([key]), required, deferred, error: None,
    };
    let host = ResidencyQueryHost {
        consume: consume_query_frame, next_child: next_query_child,
        leave: leave_query_frame, project: project_query_child,
    };
    // All frames and the original contract borrows stay with this synchronous traversal.
    unsafe { traverse_residency_query(ptr::from_mut(&mut query).cast(), host); }
    match query.error {
        Some(error) => Err(error),
        None => Ok((query.required, query.deferred)),
    }
}
pub(super) fn check_calls<'a>(calls:impl Iterator<Item=&'a Call>,known:&super::native_function::ResidencyContracts<'_>,outputs:&Outputs)->Result<bool>{
    let mut deferred=false;
    for call in calls{
        deferred=query(call,known,outputs,None,call.source.as_ref().unwrap(),call.span,deferred)?.1;
    }
    Ok(deferred)
}

#[repr(C)]
#[derive(Default)]
struct LoopRefineNode {
    condition: Context,
    yes: Context,
    no: Context,
    origins: Context,
}

#[repr(C)]
struct LoopRefineHost {
    inspect: unsafe extern "C" fn(Context) -> LoopRefineNode,
    branch: unsafe extern "C" fn(Borrowed, Context, LoopRefineNode),
    origins: unsafe extern "C" fn(Borrowed, Context, Context),
}

#[repr(C)]
struct LoopRefineOrigin {
    present: u32,
    requirements: Borrowed,
}

#[repr(C)]
struct LoopRefineOriginsHost {
    next: unsafe extern "C" fn(Context) -> LoopRefineOrigin,
    empty: unsafe extern "C" fn(Context),
    apply: unsafe extern "C" fn(Context, Borrowed),
    merge: unsafe extern "C" fn(Context),
}

#[repr(C)]
struct LoopRefineGuard {
    condition: Borrowed,
    cpu: u32,
}

#[repr(C)]
struct LoopRefineGuardsHost {
    next: unsafe extern "C" fn(Context) -> LoopRefineGuard,
    select: unsafe extern "C" fn(Context, LoopRefineGuard),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::refine_loop_residency"]
    fn refine_loop_residency(required: Borrowed, value: Context, host: LoopRefineHost);
    #[link_name = "bootstrap.ownership_path::refine_loop_origins"]
    fn refine_loop_origins(context: Context, host: LoopRefineOriginsHost);
    #[link_name = "bootstrap.ownership_path::refine_loop_guards"]
    fn refine_loop_guards(value: Context, cursor: Context, host: LoopRefineGuardsHost);
}

const LOOP_REFINE_HOST: LoopRefineHost = LoopRefineHost {
    inspect: inspect_loop_refinement,
    branch: refine_loop_branch,
    origins: refine_loop_origin_set,
};

fn refine(mut value: Residency, required: &LoopRequirements) -> Residency {
    unsafe { refine_loop_residency(ptr::from_ref(required).cast(), ptr::from_mut(&mut value).cast(), LOOP_REFINE_HOST) };
    value
}

unsafe extern "C" fn inspect_loop_refinement(value: Context) -> LoopRefineNode {
    match unsafe { &mut *value.cast::<Residency>() } {
        Residency::Selected(when, yes, no) => LoopRefineNode {
            condition: ptr::from_mut(when.as_mut()).cast(),
            yes: ptr::from_mut(yes.as_mut()).cast(),
            no: ptr::from_mut(no.as_mut()).cast(),
            ..LoopRefineNode::default()
        },
        Residency::Origins(origins) => LoopRefineNode {
            origins: ptr::from_mut(origins).cast(),
            ..LoopRefineNode::default()
        },
        Residency::Absent => LoopRefineNode::default(),
    }
}

unsafe extern "C" fn refine_loop_branch(required: Borrowed, value: Context, node: LoopRefineNode) {
    let required = unsafe { &*required.cast::<LoopRequirements>() };
    let condition = std::mem::take(unsafe { &mut *node.condition.cast::<bounds::Condition>() });
    let yes = std::mem::replace(unsafe { &mut *node.yes.cast::<Residency>() }, Residency::Absent);
    let no = std::mem::replace(unsafe { &mut *node.no.cast::<Residency>() }, Residency::Absent);
    unsafe { *value.cast::<Residency>() = Residency::conditional(condition, refine(yes, required), refine(no, required)) };
}

struct LoopRefinement<'a> {
    required: &'a LoopRequirements,
    origins: std::collections::btree_set::IntoIter<Origin>,
    current: Residency,
    result: Residency,
}

unsafe extern "C" fn refine_loop_origin_set(required: Borrowed, value: Context, origins: Context) {
    let required = unsafe { &*required.cast::<LoopRequirements>() };
    let origins = std::mem::take(unsafe { &mut *origins.cast::<BTreeSet<Origin>>() });
    let mut state = LoopRefinement {
        required,
        origins: origins.into_iter(),
        current: Residency::Absent,
        result: Residency::Absent,
    };
    unsafe { refine_loop_origins(ptr::from_mut(&mut state).cast(), LOOP_REFINE_ORIGINS_HOST) };
    unsafe { *value.cast::<Residency>() = state.result };
}

const LOOP_REFINE_ORIGINS_HOST: LoopRefineOriginsHost = LoopRefineOriginsHost {
    next: next_refinement_origin,
    empty: refine_unknown,
    apply: apply_origin_requirements,
    merge: merge_refined_origin,
};

unsafe extern "C" fn next_refinement_origin(context: Context) -> LoopRefineOrigin {
    let state = unsafe { &mut *context.cast::<LoopRefinement<'_>>() };
    let Some(origin) = state.origins.next() else {
        return LoopRefineOrigin { present: 0, requirements: ptr::null() };
    };
    let requirements = match &origin {
        Origin::Input(input) => state.required.get(input).map_or(ptr::null(), |entry| ptr::from_ref(entry).cast()),
        _ => ptr::null(),
    };
    state.current = Residency::Origins(BTreeSet::from([origin]));
    LoopRefineOrigin { present: 1, requirements }
}

unsafe extern "C" fn refine_unknown(context: Context) {
    unsafe { &mut *context.cast::<LoopRefinement<'_>>() }.result = Residency::default();
}

unsafe extern "C" fn merge_refined_origin(context: Context) {
    let state = unsafe { &mut *context.cast::<LoopRefinement<'_>>() };
    let current = std::mem::replace(&mut state.current, Residency::Absent);
    merge_bound_source(&mut state.result, current);
}

struct LoopGuardCursor<'a>(&'a mut dyn Iterator<Item = (Target, &'a bounds::Condition)>);

unsafe extern "C" fn apply_origin_requirements(context: Context, requirements: Borrowed) {
    let state = unsafe { &mut *context.cast::<LoopRefinement<'_>>() };
    let requirements = unsafe { &*requirements.cast::<BTreeMap<Target, BTreeSet<bounds::Condition>>>() };
    let mut guards = requirements.iter().flat_map(|(&target, entries)| entries.iter().map(move |guard| (target, guard)));
    let mut cursor = LoopGuardCursor(&mut guards);
    let host = LoopRefineGuardsHost { next: next_refinement_guard, select: apply_refinement_guard };
    unsafe { refine_loop_guards(ptr::from_mut(&mut state.current).cast(), ptr::from_mut(&mut cursor).cast(), host) };
}

unsafe extern "C" fn next_refinement_guard(cursor: Context) -> LoopRefineGuard {
    let cursor = unsafe { &mut *cursor.cast::<LoopGuardCursor<'_>>() };
    cursor.0.next().map_or(LoopRefineGuard { condition: ptr::null(), cpu: 0 }, |(target, condition)| {
        LoopRefineGuard { condition: ptr::from_ref(condition).cast(), cpu: u32::from(target == Target::Cpu) }
    })
}

unsafe extern "C" fn apply_refinement_guard(value: Context, guard: LoopRefineGuard) {
    let value = unsafe { &mut *value.cast::<Residency>() };
    let condition = unsafe { &*guard.condition.cast::<bounds::Condition>() };
    let physical = if guard.cpu != 0 { Residency::cpu() } else { Residency::gpu() };
    *value = Residency::conditional(condition.clone(), physical, std::mem::take(value));
}

fn physical_target(value: &Residency) -> Option<Target> {
    let Residency::Origins(origins) = value else { return None; };
    let location = |origin: &Origin| match origin {
        Origin::Cpu | Origin::TaskTransfer => Some(Target::Cpu),
        Origin::Gpu => Some(Target::Gpu),
        _ => None,
    };
    let first = origins.first().and_then(location)?;
    origins.iter().all(|origin| location(origin) == Some(first)).then_some(first)
}
type Borrowed = *const c_void;
type Context = *mut c_void;

#[repr(u32)]
#[derive(Clone, Copy)]
enum ResidencyCheckMode { Known, Require, Query }

#[repr(C)]
#[derive(Clone, Copy)]
struct ResidencyVisit {
    value: Borrowed,
    path: Borrowed,
    wanted: Borrowed,
    target: u32,
    mode: ResidencyCheckMode,
    deferred: u32,
}

#[repr(C)]
struct ResidencyVisitResult { accepted: u32, deferred: u32 }

#[repr(C)]
struct ResidencyCheckNode { branch: ResidencyBranch, origins: Borrowed }

#[repr(u32)]
enum ResidencyOriginKind { End, Cpu, Gpu, Input, Output }

#[repr(C)]
struct ResidencyOriginItem { kind: ResidencyOriginKind, value: Borrowed }

#[repr(C)]
struct ResidencyCheckHost {
    possible: unsafe extern "C" fn(Borrowed) -> u32,
    inspect: unsafe extern "C" fn(Borrowed) -> ResidencyCheckNode,
    branch: unsafe extern "C" fn(Context, ResidencyVisit, Borrowed, u32) -> ResidencyVisitResult,
    origins: unsafe extern "C" fn(Context, ResidencyVisit, Borrowed) -> ResidencyVisitResult,
}

#[repr(C)]
struct ResidencyOriginsHost {
    next: unsafe extern "C" fn(Context) -> ResidencyOriginItem,
    overlaps: unsafe extern "C" fn(Borrowed) -> u32,
    same_input: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    insert: unsafe extern "C" fn(Context, Borrowed, u32, Borrowed) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::check_residency"]
    fn check_residency(context: Context, visit: ResidencyVisit, host: ResidencyCheckHost) -> ResidencyVisitResult;
    #[link_name = "bootstrap.ownership_path::check_residency_origins"]
    fn check_residency_origins(context: Context, visit: ResidencyVisit, iterator: Context, host: ResidencyOriginsHost) -> ResidencyVisitResult;
}

unsafe extern "C" fn inspect_residency_visit(value: Borrowed) -> ResidencyCheckNode {
    let origins = match unsafe { &*value.cast::<Residency>() } {
        Residency::Origins(origins) => ptr::from_ref(origins).cast(),
        _ => ptr::null(),
    };
    ResidencyCheckNode { branch: unsafe { compare_selected(value) }, origins }
}

unsafe extern "C" fn visit_residency_branch(context: Context, mut visit: ResidencyVisit, condition: Borrowed, truth: u32) -> ResidencyVisitResult {
    let path = unsafe { &*condition.cast::<bounds::Condition>() }
        .assume(unsafe { &*visit.path.cast::<bounds::Clause>() }, truth != 0);
    visit.path = ptr::from_ref(&path).cast();
    unsafe { check_residency(context, visit, residency_check_host()) }
}

unsafe extern "C" fn visit_next_origin(iterator: Context) -> ResidencyOriginItem {
    use ResidencyOriginKind::{Cpu, End, Gpu, Input, Output};
    let iterator = unsafe { &mut *iterator.cast::<std::collections::btree_set::Iter<'_, Origin>>() };
    let (kind, value) = match iterator.next() {
        Some(origin @ (Origin::Cpu | Origin::TaskTransfer)) => (Cpu, ptr::from_ref(origin).cast()),
        Some(origin @ Origin::Gpu) => (Gpu, ptr::from_ref(origin).cast()),
        Some(Origin::Input(input)) => (Input, ptr::from_ref(input).cast()),
        Some(Origin::Output(output)) => (Output, ptr::from_ref(output).cast()),
        None => (End, ptr::null()),
    };
    ResidencyOriginItem { kind, value }
}

unsafe extern "C" fn visit_path_overlaps(path: Borrowed) -> u32 {
    u32::from(unsafe { &*path.cast::<bounds::Clause>() }.overlaps(&bounds::Clause::default()))
}

unsafe extern "C" fn visit_same_input(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe { &*left.cast::<Input>() == &*right.cast::<Input>() })
}

unsafe extern "C" fn visit_insert_requirement(context: Context, input: Borrowed, target: u32, path: Borrowed) -> u32 {
    let required = unsafe { &mut *context.cast::<Requirements>() };
    let input = unsafe { &*input.cast::<Input>() };
    let target = match target { 0 => Target::Cpu, 1 => Target::Gpu, _ => Target::Compute };
    let path = unsafe { &*path.cast::<bounds::Clause>() };
    u32::from(insert(required, input.clone(), target, bounds::Condition::guard(path.clone())))
}

unsafe extern "C" fn visit_residency_origins(context: Context, visit: ResidencyVisit, origins: Borrowed) -> ResidencyVisitResult {
    let mut iterator = unsafe { &*origins.cast::<BTreeSet<Origin>>() }.iter();
    let host = ResidencyOriginsHost {
        next: visit_next_origin, overlaps: visit_path_overlaps,
        same_input: visit_same_input, insert: visit_insert_requirement,
    };
    unsafe { check_residency_origins(context, visit, ptr::from_mut(&mut iterator).cast(), host) }
}

fn residency_check_host() -> ResidencyCheckHost {
    ResidencyCheckHost {
        possible: compare_possible,
        inspect: inspect_residency_visit,
        branch: visit_residency_branch,
        origins: visit_residency_origins,
    }
}

impl ResidencyVisit {
    fn new(value: &Residency, target: Target, path: &bounds::Clause, mode: ResidencyCheckMode) -> Self {
        Self {
            value: ptr::from_ref(value).cast(),
            path: ptr::from_ref(path).cast(),
            wanted: ptr::null(),
            target: match target { Target::Cpu => 0, Target::Gpu => 1, Target::Compute => 2 },
            mode,
            deferred: 0,
        }
    }

    unsafe fn run(self, context: Context) -> ResidencyVisitResult {
        unsafe { check_residency(context, self, residency_check_host()) }
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ResidencyPair {
    left: Borrowed,
    right: Borrowed,
    path: Borrowed,
}

#[repr(C)]
struct ResidencyBranch {
    condition: Borrowed,
    yes: Borrowed,
    no: Borrowed,
}

#[repr(u32)]
enum ChoiceAction { Absent, Yes, No, Join, Select }
#[repr(u32)]
enum JoinAction { Keep, Copy, Clear, Extend, Select }

#[repr(C)]
struct ChoicePlan {
    action: ChoiceAction,
    yes_depth: usize,
    no_depth: usize,
    split_yes: u32,
    split_no: u32,
}

#[repr(C)]
struct JoinPlan {
    action: JoinAction,
    split_left: u32,
    condition: Borrowed,
    right_yes: Borrowed,
    right_no: Borrowed,
}

#[repr(C)]
struct ChoiceFacts { impossible: u32, literal: u32, unconstrained: u32 }

#[repr(C)]
struct ResidencyAlgebraHost {
    inspect: unsafe extern "C" fn(Borrowed) -> ResidencyCheckNode,
    equal: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    condition_equal: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    order: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    facts: unsafe extern "C" fn(Borrowed) -> ChoiceFacts,
    exclusive: unsafe extern "C" fn(Borrowed) -> u32,
    empty: unsafe extern "C" fn(Borrowed) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::choose_residency"]
    fn choose_residency(when: Borrowed, yes: Borrowed, no: Borrowed, host: ResidencyAlgebraHost) -> ChoicePlan;
    #[link_name = "bootstrap.ownership_path::join_residency"]
    fn join_residency(left: Borrowed, right: Borrowed, host: ResidencyAlgebraHost) -> JoinPlan;
}

unsafe extern "C" fn algebra_equal(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe { *left.cast::<Residency>() == *right.cast::<Residency>() })
}

unsafe extern "C" fn algebra_condition_equal(left: Borrowed, right: Borrowed) -> u32 {
    u32::from(unsafe { *left.cast::<bounds::Condition>() == *right.cast::<bounds::Condition>() })
}

unsafe extern "C" fn algebra_order(left: Borrowed, right: Borrowed) -> i32 {
    unsafe { (&*left.cast::<bounds::Condition>()).cmp(&*right.cast::<bounds::Condition>()) as i32 }
}

unsafe extern "C" fn algebra_facts(value: Borrowed) -> ChoiceFacts {
    let value = unsafe { &*value.cast::<bounds::Condition>() };
    ChoiceFacts {
        impossible: u32::from(value.impossible()),
        literal: value.boolean_constant().map_or(2, u32::from),
        unconstrained: u32::from(value.unconstrained()),
    }
}

unsafe extern "C" fn algebra_exclusive(value: Borrowed) -> u32 {
    u32::from(unsafe { &*value.cast::<bounds::Condition>() }.exclusive())
}

unsafe extern "C" fn algebra_empty(origins: Borrowed) -> u32 {
    u32::from(unsafe { &*origins.cast::<BTreeSet<Origin>>() }.is_empty())
}

fn residency_algebra_host() -> ResidencyAlgebraHost {
    ResidencyAlgebraHost {
        inspect: inspect_residency_visit,
        equal: algebra_equal,
        condition_equal: algebra_condition_equal,
        order: algebra_order,
        facts: algebra_facts,
        exclusive: algebra_exclusive,
        empty: algebra_empty,
    }
}

#[repr(C)]
struct ResidencyCompareHost {
    possible: unsafe extern "C" fn(Borrowed) -> u32,
    equivalent: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    selected: unsafe extern "C" fn(Borrowed) -> ResidencyBranch,
    project: unsafe extern "C" fn(Context, ResidencyPair) -> u32,
    branch: unsafe extern "C" fn(Context, ResidencyPair, Borrowed, u32) -> u32,
    leaf: unsafe extern "C" fn(Context, ResidencyPair) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::compare_residency"]
    fn compare_residency(context: Context, pair: ResidencyPair, host: ResidencyCompareHost) -> u32;
    #[link_name = "bootstrap.ownership_path::compare_projected_residency"]
    fn compare_projected_residency(context: Context, pair: ResidencyPair, host: ResidencyCompareHost) -> u32;
}

unsafe extern "C" fn compare_possible(path: Borrowed) -> u32 {
    u32::from(unsafe { &*path.cast::<bounds::Clause>() }.possible())
}

unsafe extern "C" fn compare_equivalent(left: Borrowed, right: Borrowed) -> u32 {
    let left = unsafe { &*left.cast::<Residency>() };
    let right = unsafe { &*right.cast::<Residency>() };
    u32::from(left == right || physical_target(left).is_some_and(|target| physical_target(right) == Some(target)))
}

unsafe extern "C" fn compare_selected(value: Borrowed) -> ResidencyBranch {
    match unsafe { &*value.cast::<Residency>() } {
        Residency::Selected(condition, yes, no) => ResidencyBranch {
            condition: ptr::from_ref(condition.as_ref()).cast(),
            yes: ptr::from_ref(yes.as_ref()).cast(),
            no: ptr::from_ref(no.as_ref()).cast(),
        },
        _ => ResidencyBranch { condition: ptr::null(), yes: ptr::null(), no: ptr::null() },
    }
}

unsafe extern "C" fn compare_project<F>(context: Context, pair: ResidencyPair) -> u32
where
    F: FnMut(&Residency, &Residency, &bounds::Clause) -> bool,
{
    fn under<'a>(value: &'a Residency, path: &bounds::Clause) -> Cow<'a, Residency> {
        match value {
            Residency::Selected(..) => Cow::Owned(value.under(path)),
            _ => Cow::Borrowed(value),
        }
    }
    let left = under(unsafe { &*pair.left.cast::<Residency>() }, unsafe { &*pair.path.cast() });
    let right = under(unsafe { &*pair.right.cast::<Residency>() }, unsafe { &*pair.path.cast() });
    let projected = ResidencyPair {
        left: ptr::from_ref(left.as_ref()).cast(),
        right: ptr::from_ref(right.as_ref()).cast(),
        path: pair.path,
    };
    unsafe { compare_projected_residency(context, projected, compare_host::<F>()) }
}

unsafe extern "C" fn compare_branch<F>(context: Context, mut pair: ResidencyPair, condition: Borrowed, truth: u32) -> u32
where
    F: FnMut(&Residency, &Residency, &bounds::Clause) -> bool,
{
    let condition = unsafe { &*condition.cast::<bounds::Condition>() };
    let path = condition.assume(unsafe { &*pair.path.cast() }, truth != 0);
    pair.path = ptr::from_ref(&path).cast();
    unsafe { compare_residency(context, pair, compare_host::<F>()) }
}

unsafe extern "C" fn compare_leaf<F>(context: Context, pair: ResidencyPair) -> u32
where
    F: FnMut(&Residency, &Residency, &bounds::Clause) -> bool,
{
    let leaf = unsafe { &mut *context.cast::<F>() };
    u32::from(leaf(unsafe { &*pair.left.cast() }, unsafe { &*pair.right.cast() }, unsafe { &*pair.path.cast() }))
}

fn compare_host<F>() -> ResidencyCompareHost
where
    F: FnMut(&Residency, &Residency, &bounds::Clause) -> bool,
{
    ResidencyCompareHost {
        possible: compare_possible,
        equivalent: compare_equivalent,
        selected: compare_selected,
        project: compare_project::<F>,
        branch: compare_branch::<F>,
        leaf: compare_leaf::<F>,
    }
}

fn equal_under<F>(left: &Residency, right: &Residency, path: &bounds::Clause, leaf: &mut F) -> bool
where
    F: FnMut(&Residency, &Residency, &bounds::Clause) -> bool,
{
    let pair = ResidencyPair {
        left: ptr::from_ref(left).cast(),
        right: ptr::from_ref(right).cast(),
        path: ptr::from_ref(path).cast(),
    };
    unsafe { compare_residency(ptr::from_mut(leaf).cast(), pair, compare_host::<F>()) != 0 }
}
pub(super) fn check_loops(
    name: &str,
    inputs: &Inputs,
    contract: &Contract,
    parameters: &BTreeMap<ValueIdentity, bounds::Proof>,
    outputs: &Outputs,
    known: &super::native_function::ResidencyContracts<'_>,
    parameter_count: usize,
) -> Result<()> {
    let backedges = inputs.uses.iter().filter_map(|usage| match usage {
        Use::Same { before, after, path, source, span } => Some((before, after, path, source, *span)),
        _ => None,
    });
    if backedges.clone().next().is_none() {
        return Ok(());
    }

    let mut required = contract.required.clone();
    let mut root = None;
    for (before, after, _, source, span) in backedges.clone() {
        let mut wanted = BTreeSet::new();
        self::inputs(&evaluate(before, outputs, &bounds::Clause::default()), &mut wanted);
        self::inputs(&evaluate(after, outputs, &bounds::Clause::default()), &mut wanted);
        for input in wanted {
            let input = input.at_call(parameters);
            let call = root.get_or_insert_with(|| Call {
                function: name.into(),
                arguments: Arc::new((0..parameter_count).map(|parameter| (Input::new(parameter), Residency::parameter(parameter))).collect()),
                bindings: parameters.values().filter_map(|proof| proof.known_identity(0))
                    .map(|id| (id, bounds::Proof::default().with_identity(id))).collect(),
                path: bounds::Clause::default(), source: None, span: Span::default(),
            });
            let (queried, _) = query(call, known, outputs, Some(&input), source, span, false)?;
            for (key, guards) in queried {
                required.entry(key).or_default().extend(guards);
            }
        }
    }
    let required = local_loop_requirements(required, parameters);
    for (before, after, path, source, span) in backedges {
        let before = evaluate(before, outputs, path);
        let after = evaluate(after, outputs, path);
        if before.absent() || after.absent() {
            continue;
        }
        if !equal_under(&refine(before, &required), &refine(after, &required), path,
            &mut |_, _, path| !path.overlaps(&bounds::Clause::default())) {
            return source.fail(span, "B266", "循环真实回边没有在该路径归还入口相同的CPU／GPU存储位置");
        }
    }
    Ok(())
}
pub(super) fn visit(
    value: &mut Returned,
    input: &mut Input,
    apply: &mut impl FnMut(&mut application::Data, &Input),
) {
    let root = unsafe { inspect_availability(ptr::from_mut(value).cast()) };
    visit_path(root, input, apply);
}

struct PathVisit<'a> {
    input: &'a mut Input,
    apply: &'a mut dyn FnMut(&mut application::Data, &Input),
}

fn visit_path(
    root: AvailabilityNode,
    input: &mut Input,
    apply: &mut dyn FnMut(&mut application::Data, &Input),
) {
    let mut visit = PathVisit { input, apply };
    unsafe { walk_resource_paths(ptr::from_mut(&mut visit).cast(), root, EmptyElements::Skip, MUTABLE_PATH_HOST) };
}

const MUTABLE_PATH_HOST: ResourcePathHost = ResourcePathHost {
    dataset: visit_path_dataset,
    maximum: dataset_maximum,
    element: dataset_element,
    field: availability_field,
    edge: visit_path_edge,
};

unsafe extern "C" fn visit_path_dataset(context: Context, model: Context) {
    let visit = unsafe { &mut *context.cast::<PathVisit<'_>>() };
    let data = unsafe { &mut *model.cast::<application::Data>() };
    (visit.apply)(data, visit.input);
}

unsafe extern "C" fn visit_path_edge(context: Context, part: PathPart, index: usize, child: Context) {
    let visit = unsafe { &mut *context.cast::<PathVisit<'_>>() };
    visit.input.fields.push(part.at(index));
    let node = unsafe { inspect_availability(child) };
    unsafe { walk_resource_paths(ptr::from_mut(visit).cast(), node, EmptyElements::Skip, MUTABLE_PATH_HOST) };
    visit.input.fields.pop();
}

// Only location facts are selected here; ordinary value and borrow joins remain owned by Returned.
pub(super) fn model_location<'a>(mut model:&'a Returned,path:&[Part])->Option<&'a application::Data>{
    for part in path{model=match(part,model){
        (Part::Field(field),Returned::OwnedValue(value))=>value.payloads[*field].as_deref()?,
        (Part::Elements,Returned::Dataset(data))=>data.elements.known()?,
        _=>return None,
    };}
    if let Returned::Dataset(data)=model{Some(data)}else{None}
}
pub(super) fn select_elements(target:&mut Returned,incoming:&Returned,previous:&Returned,choice:&bounds::Condition,indices:usize){
    visit(target,&mut Input::new(0),&mut|data,input|{
        let depth=input.fields.iter().filter(|part|matches!(part,Part::Elements)).count();
        let shifts=(0..indices).map(|i|(ValueIdentity::ElementIndex(i),ValueIdentity::ElementIndex(i+depth))).collect();
        let choice=choice.output_bound(&shifts);
        data.residency=match(model_location(incoming,&input.fields),model_location(previous,&input.fields)){
            (Some(a),Some(b))=>Residency::conditional(choice,a.residency.clone(),b.residency.clone()),
            (Some(a),None)=>Residency::conditional(choice,a.residency.clone(),Residency::Absent),
            (None,Some(b))=>Residency::conditional(choice,Residency::Absent,b.residency.clone()),
            (None,None)=>Residency::Absent,
        };
    });
}
pub(super) fn parameter(value: &mut owned_values::OwnedValue, parameter: usize) {
    for (resource,path) in view::resource_paths(&value.value.ty).into_iter().enumerate() {
        if let Some(data)=view::owned_data_at_mut(value,&path) {
            data.root.proof.set_view_borrow(0,view::Borrow::source(view::Source::Data(ValueIdentity::ResourceParameter(parameter,resource))));
            data.residency=Residency::input(Input{parameter,fields:path.into_iter().map(Part::Field).collect()});
        }
    }
}
// An empty dataset still has its own location, but has no element resource.
fn argument_data(arguments:&mut Arguments,input:&Input,data:&application::Data){
    arguments.insert(input.clone(),data.residency.clone());
    if data.length.maximum()==0{
        let mut contents=input.clone();contents.fields.push(Part::Elements);
        arguments.insert(contents,Residency::Absent);
    }
}
pub(super) fn argument(arguments: &mut Arguments, parameter: usize, value: &mut application::Data, projection: &[usize]) {
    argument_data(arguments, &Input::new(parameter), value);
    if value.length.maximum() == 0 {
        return;
    }
    let mut elements = value.elements.known_mut();
    for &field in projection {
        elements = match elements {
            Some(Returned::OwnedValue(value)) => value.payloads[field].as_deref_mut(),
            _ => None,
        };
    }
    if let Some(elements) = elements {
        visit(elements, &mut Input { parameter, fields: vec![Part::Elements] }, &mut |data, input| {
            argument_data(arguments, input, data);
        });
    }
}
pub(super) fn owned_argument(
    arguments: &mut Arguments,
    parameter: usize,
    value: &mut owned_values::OwnedValue,
) {
    visit_path(owned_availability(value), &mut Input::new(parameter), &mut |data, input| {
        argument_data(arguments, input, data);
    });
}
// 这里只交接载荷存在性。位置继续由同一调用的输出路径消费，不能先折成父位置。
unsafe extern "C" fn bound_dataset_available(arguments: Borrowed, model: Context) -> u32 {
    let arguments = unsafe { &*arguments.cast::<Arguments>() };
    let data = unsafe { &mut *model.cast::<application::Data>() };
    let mut scratch = Residency::Absent;
    if data.residency.bind_into(arguments, &mut scratch, false) {
        1
    } else {
        data.residency = Residency::default();
        0
    }
}

pub(super) fn returned(value: &mut Returned, arguments: Option<&Arguments>) {
    let empty = Arguments::new();
    let arguments = arguments.unwrap_or(&empty);
    let host = OutputAvailabilityHost { present: bound_dataset_available, ..OUTPUT_AVAILABILITY_HOST };
    unsafe { resolve_output_availability(ptr::from_ref(arguments).cast(), ptr::from_mut(value).cast(), host); }
}





// 编译期调用结果是原资源来源表达式，不形成运行时位置表。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum OutputKind {Returned,Written(usize)}
#[derive(Clone)]
pub(super) struct Output {
    function: String,
    kind: OutputKind,
    fields: Vec<Part>,
    arguments: SharedArguments,
    bindings: BTreeMap<ValueIdentity,bounds::Proof>,
}
impl PartialEq for Output{fn eq(&self,other:&Self)->bool{self.cmp(other)==std::cmp::Ordering::Equal}}
impl Eq for Output{}
impl PartialOrd for Output{fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl Ord for Output{fn cmp(&self,other:&Self)->std::cmp::Ordering{use crate::module::codec::Encode;(self.function.as_str(),&self.kind,&self.fields,&self.arguments).cmp(&(other.function.as_str(),&other.kind,&other.fields,&other.arguments)).then_with(||self.bindings.iter().map(|(id,proof)|(*id,proof.bytes())).cmp(other.bindings.iter().map(|(id,proof)|(*id,proof.bytes()))))}}

pub(super) enum Use {
    At {
        value: Residency,
        path: bounds::Clause,
        target: Target,
        source: Source,
        span: Span,
    },
    Same {
        before: Residency,
        after: Residency,
        path: bounds::Clause,
        source: Source,
        span: Span,
    },
}

type OutputPaths = BTreeMap<Vec<Part>, Residency>;
pub(super) type Outputs = BTreeMap<String, BTreeMap<OutputKind, OutputPaths>>;
#[repr(u32)]
enum DependencyItemKind { End, Reference, Model, Residency, Concrete }

#[repr(C)]
struct DependencyItem {
    kind: DependencyItemKind,
    value: Borrowed,
    arguments: Borrowed,
}

impl DependencyItem {
    fn node<T>(kind: DependencyItemKind, value: &T) -> Self {
        Self { kind, value: ptr::from_ref(value).cast(), arguments: ptr::null() }
    }

    fn reference(name: &String, arguments: &Arguments) -> Self {
        Self {
            kind: DependencyItemKind::Reference,
            value: ptr::from_ref(name).cast(),
            arguments: ptr::from_ref(arguments).cast(),
        }
    }
}

#[repr(u32)]
enum DependencyList { Arguments, Origins, Payloads }

#[repr(C)]
struct ReturnedResidency {
    residency: Borrowed,
    elements: Borrowed,
    payloads: Borrowed,
    maximum: u64,
}

#[repr(C)]
struct DependencyHost {
    next: unsafe extern "C" fn(Context) -> DependencyItem,
    insert: unsafe extern "C" fn(Context, Borrowed),
    model: unsafe extern "C" fn(Borrowed) -> ReturnedResidency,
    residency: unsafe extern "C" fn(Borrowed) -> ResidencyCheckNode,
    walk: unsafe extern "C" fn(Context, DependencyList, Borrowed),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::collect_residency_dependencies"]
    fn collect_residency_dependencies(names: Context, cursor: Context, host: DependencyHost);
}

type DependencyCursor<'a> = &'a mut dyn Iterator<Item = DependencyItem>;

unsafe extern "C" fn next_dependency(cursor: Context) -> DependencyItem {
    let cursor = unsafe { &mut *cursor.cast::<DependencyCursor<'_>>() };
    cursor.next().unwrap_or(DependencyItem {
        kind: DependencyItemKind::End, value: ptr::null(), arguments: ptr::null(),
    })
}

unsafe extern "C" fn insert_dependency(names: Context, name: Borrowed) {
    let names = unsafe { &mut *names.cast::<BTreeSet<String>>() };
    names.insert(unsafe { &*name.cast::<String>() }.clone());
}

unsafe extern "C" fn inspect_returned_residency(model: Borrowed) -> ReturnedResidency {
    let mut result = ReturnedResidency {
        residency: ptr::null(), elements: ptr::null(), payloads: ptr::null(), maximum: 0,
    };
    match unsafe { &*model.cast::<Returned>() } {
        Returned::Dataset(data) => {
            result.residency = ptr::from_ref(&data.residency).cast();
            result.elements = data.elements.known().map_or(ptr::null(), |element| ptr::from_ref(element).cast());
            result.maximum = u64::from(data.length.maximum());
        }
        Returned::OwnedValue(value) => result.payloads = ptr::from_ref(value).cast(),
        _ => {}
    }
    result
}

unsafe extern "C" fn walk_dependency_list(names: Context, list: DependencyList, value: Borrowed) {
    match list {
        DependencyList::Arguments => {
            let arguments = unsafe { &*value.cast::<Arguments>() };
            walk_dependencies(names, arguments.values().map(|value| DependencyItem::node(DependencyItemKind::Residency, value)));
        }
        DependencyList::Origins => {
            let origins = unsafe { &*value.cast::<BTreeSet<Origin>>() };
            walk_dependencies(names, origins.iter().map(|origin| match origin {
                Origin::Output(output) => DependencyItem::reference(&output.function, &output.arguments),
                _ => DependencyItem::node(DependencyItemKind::Concrete, origin),
            }));
        }
        DependencyList::Payloads => {
            let owned = unsafe { &*value.cast::<owned_values::OwnedValue>() };
            walk_dependencies(names, owned.payloads.iter().filter_map(|payload| {
                payload.as_deref().map(|model| DependencyItem::node(DependencyItemKind::Model, model))
            }));
        }
    }
}

fn walk_dependencies(names: Context, mut items: impl Iterator<Item = DependencyItem>) {
    let mut cursor: DependencyCursor<'_> = &mut items;
    let host = DependencyHost {
        next: next_dependency, insert: insert_dependency, model: inspect_returned_residency,
        residency: inspect_residency_visit, walk: walk_dependency_list,
    };
    // The B walk completes while the typed iterators and original contracts are borrowed.
    unsafe { collect_residency_dependencies(names, ptr::from_mut(&mut cursor).cast(), host) }
}

pub(super) fn dependencies<'a>(contract: &Contract, models: impl Iterator<Item = &'a Returned>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let calls = contract.calls.iter().map(|call| DependencyItem::reference(&call.function, &call.arguments));
    let models = models.map(|model| DependencyItem::node(DependencyItemKind::Model, model));
    walk_dependencies(ptr::from_mut(&mut names).cast(), calls.chain(models));
    names
}
#[repr(C)]
struct ResidencyPrefix {
    residency: Borrowed,
    fields: usize,
}

struct OutputLookup<'a> {
    paths: &'a OutputPaths,
    fields: &'a [Part],
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::find_residency_prefix"]
    fn find_residency_prefix(
        context: Context,
        fields: usize,
        lookup: unsafe extern "C" fn(Context, usize) -> Borrowed,
    ) -> ResidencyPrefix;
}

unsafe extern "C" fn lookup_output_path(context: Context, fields: usize) -> Borrowed {
    let lookup = unsafe { &*context.cast::<OutputLookup<'_>>() };
    lookup.paths.get(&lookup.fields[..fields]).map_or(ptr::null(), |value| ptr::from_ref(value).cast())
}

fn output_source(output: &Output, outputs: &Outputs) -> Option<Residency> {
    let paths = outputs[&output.function].get(&output.kind)?;
    let mut lookup = OutputLookup { paths, fields: &output.fields };
    let fields = lookup.fields.len();
    let prefix = unsafe {
        find_residency_prefix(ptr::from_mut(&mut lookup).cast(), fields, lookup_output_path)
    };
    let value = unsafe { prefix.residency.cast::<Residency>().as_ref() }?;
    if value.absent() {
        return None;
    }
    let fields = path_model(&output.fields);
    let depth = fields.iter().filter(|part| !matches!(part, Part::Field(_))).count();
    let selection = path_selection(&fields);
    let bindings = scoped_indices(
        output.bindings.iter().filter(|(id, _)| !matches!(id, ValueIdentity::ElementIndex(_))),
        depth,
    );
    let arguments = output.arguments.iter().map(|(input, value)| {
        (input.clone(), value.scope_indices(depth, input.depth()))
    }).collect();
    let fields = &fields[prefix.fields..];
    let removed = fields.iter().filter(|part| !matches!(part, Part::Field(_))).count();
    let value = value.scope_indices(removed, 0).project(fields, &selection).at_call(&bindings);
    let selected = output.bindings.iter()
        .filter(|(id, _)| matches!(id, ValueIdentity::ElementIndex(index) if *index < depth))
        .map(|(id, proof)| (*id, proof.clone())).collect();
    value.element_indices(&selected).bind(&arguments).map(|value| {
        let outer = value.index_names().into_iter().filter_map(|id| match id {
            ValueIdentity::ElementIndex(index) if index >= depth => Some((
                id, bounds::Proof::default().with_identity(ValueIdentity::ElementIndex(index - depth)),
            )),
            _ => None,
        }).collect();
        value.element_indices(&outer)
    })
}
// An active output refers to the actual bound resource and value snapshots.
fn output_source_key(output: &Output) -> Vec<u8> {
    use crate::module::codec::{Encode, Writer};

    let mut w = Writer(vec![3]);
    output.function.encode(&mut w);
    output.kind.encode(&mut w);
    output.fields.encode(&mut w);
    output.arguments.as_ref().encode(&mut w);
    query_bindings(&output.bindings, &mut w);
    w.0
}

// Follow a single output source without replacing its identity with its storage.
fn canonical_output_key(output: &Output, outputs: &Outputs) -> Vec<u8> {
    use std::collections::btree_map::Entry;
    let mut output = Cow::Borrowed(output);
    let mut visited = BTreeMap::new();
    loop {
        let entry = match visited.entry(output_source_key(&output)) {
            Entry::Occupied(entry) => return entry.remove_entry().0,
            Entry::Vacant(entry) => entry,
        };
        let Some(Residency::Origins(mut origins)) = output_source(&output, outputs) else {
            return entry.into_key();
        };
        if origins.len() != 1 {
            return entry.into_key();
        }
        let Some(Origin::Output(next)) = origins.pop_first() else {
            return entry.into_key();
        };
        entry.insert(());
        output = Cow::Owned(next);
    }
}

struct SourceResolution<'a> {
    outputs: &'a Outputs,
    active: BTreeSet<Vec<u8>>,
}

#[repr(C)]
struct SourceVisit {
    value: Borrowed,
    path: Borrowed,
    result: *mut Residency,
}

impl SourceVisit {
    fn new(value: &Residency, path: &bounds::Clause, result: *mut Residency) -> Self {
        Self { value: ptr::from_ref(value).cast(), path: ptr::from_ref(path).cast(), result }
    }
}

#[repr(C)]
struct ResidencySourceHost {
    possible: unsafe extern "C" fn(Borrowed) -> u32,
    selected: unsafe extern "C" fn(Borrowed) -> ResidencyBranch,
    truth: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    under: unsafe extern "C" fn(Context, SourceVisit, Borrowed, u32) -> u32,
    both: unsafe extern "C" fn(Context, SourceVisit, ResidencyBranch),
    origins: unsafe extern "C" fn(Context, SourceVisit) -> u32,
}

#[repr(u32)]
enum ResourceOriginKind { End, Concrete, Output }

#[repr(C)]
struct ResourceOrigin {
    kind: ResourceOriginKind,
    value: Borrowed,
}

#[repr(C)]
struct ResourceOriginHost {
    next: unsafe extern "C" fn(Context) -> ResourceOrigin,
    append: unsafe extern "C" fn(Context, Borrowed),
    output: unsafe extern "C" fn(Context, Borrowed) -> u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::resolve_residency_source"]
    fn resolve_residency_source(context: Context, visit: SourceVisit, host: ResidencySourceHost) -> u32;
    #[link_name = "bootstrap.ownership_path::resolve_viable_source"]
    fn resolve_viable_source(context: Context, visit: SourceVisit, host: ResidencySourceHost) -> u32;
    #[link_name = "bootstrap.ownership_path::resolve_resource_origins"]
    fn resolve_resource_origins(context: Context, collect: u32, host: ResourceOriginHost) -> u32;
}

const SOURCE_HOST: ResidencySourceHost = ResidencySourceHost {
    possible: compare_possible,
    selected: compare_selected,
    truth: source_truth,
    under: source_under,
    both: source_choice,
    origins: source_origins,
};

unsafe extern "C" fn source_truth(condition: Borrowed, path: Borrowed) -> u32 {
    unsafe { &*condition.cast::<bounds::Condition>() }
        .truth_in(unsafe { &*path.cast::<bounds::Clause>() }).map_or(2, u32::from)
}

unsafe extern "C" fn source_under(context: Context, mut visit: SourceVisit, condition: Borrowed, truth: u32) -> u32 {
    let condition = unsafe { &*condition.cast::<bounds::Condition>() };
    let path = condition.assume(unsafe { &*visit.path.cast::<bounds::Clause>() }, truth != 0);
    visit.path = ptr::from_ref(&path).cast();
    unsafe { resolve_residency_source(context, visit, SOURCE_HOST) }
}

unsafe extern "C" fn source_choice(context: Context, visit: SourceVisit, branch: ResidencyBranch) {
    let mut yes = Residency::Absent;
    let mut no = Residency::Absent;
    unsafe {
        source_under(context, SourceVisit { value: branch.yes, path: visit.path, result: &mut yes }, branch.condition, 1);
        source_under(context, SourceVisit { value: branch.no, path: visit.path, result: &mut no }, branch.condition, 0);
        let condition = &*branch.condition.cast::<bounds::Condition>();
        *visit.result = Residency::conditional(condition.clone(), yes, no);
    }
}

struct SourceOrigins<'a> {
    resolution: Context,
    iter: std::collections::btree_set::Iter<'a, Origin>,
    path: Borrowed,
    result: *mut Residency,
}

unsafe extern "C" fn source_origins(context: Context, visit: SourceVisit) -> u32 {
    let value = unsafe { &*visit.value.cast::<Residency>() };
    let Residency::Origins(origins) = value else { return 0; };
    if origins.is_empty() {
        if let Some(result) = unsafe { visit.result.as_mut() } {
            *result = Residency::default();
        }
        return 1;
    }
    if let Some(result) = unsafe { visit.result.as_mut() }
        && !origins.iter().any(|origin| matches!(origin, Origin::Output(_)))
    {
        *result = value.clone();
        return 1;
    }
    let mut cursor = SourceOrigins { resolution: context, iter: origins.iter(), path: visit.path, result: visit.result };
    let host = ResourceOriginHost { next: next_source_origin, append: append_source_origin, output: resolve_output_origin };
    unsafe { resolve_resource_origins(ptr::from_mut(&mut cursor).cast(), u32::from(!visit.result.is_null()), host) }
}

unsafe extern "C" fn next_source_origin(context: Context) -> ResourceOrigin {
    let cursor = unsafe { &mut *context.cast::<SourceOrigins<'_>>() };
    match cursor.iter.next() {
        None => ResourceOrigin { kind: ResourceOriginKind::End, value: ptr::null() },
        Some(Origin::Output(output)) => ResourceOrigin { kind: ResourceOriginKind::Output, value: ptr::from_ref(output).cast() },
        Some(origin) => ResourceOrigin { kind: ResourceOriginKind::Concrete, value: ptr::from_ref(origin).cast() },
    }
}

unsafe extern "C" fn append_source_origin(context: Context, origin: Borrowed) {
    let cursor = unsafe { &mut *context.cast::<SourceOrigins<'_>>() };
    let origin = unsafe { &*origin.cast::<Origin>() };
    unsafe { &mut *cursor.result }.join(&Residency::Origins(BTreeSet::from([origin.clone()])));
}

unsafe extern "C" fn resolve_output_origin(context: Context, output: Borrowed) -> u32 {
    let cursor = unsafe { &mut *context.cast::<SourceOrigins<'_>>() };
    let output = unsafe { &*output.cast::<Output>() };
    let resolution = unsafe { &mut *cursor.resolution.cast::<SourceResolution<'_>>() };
    let key = output_source_key(output);
    if !resolution.active.insert(key.clone()) {
        return 0;
    }
    let value = output_source(output, resolution.outputs);
    let mut found = 0;
    if let Some(value) = value {
        let mut projected = Residency::Absent;
        let result = if cursor.result.is_null() { ptr::null_mut() } else { ptr::from_mut(&mut projected) };
        let visit = SourceVisit { value: ptr::from_ref(&value).cast(), path: cursor.path, result };
        found = unsafe { resolve_viable_source(cursor.resolution, visit, SOURCE_HOST) };
        if let Some(result) = unsafe { cursor.result.as_mut() } {
            result.join(&projected);
        }
    }
    unsafe { &mut *cursor.resolution.cast::<SourceResolution<'_>>() }.active.remove(&key);
    found
}

fn present(value: &Residency, outputs: &Outputs) -> bool {
    let mut resolution = SourceResolution { outputs, active: BTreeSet::new() };
    let path = bounds::Clause::default();
    let visit = SourceVisit::new(value, &path, ptr::null_mut());
    // The empty path is viable; borrowed nodes, branches and active outputs live for this call.
    unsafe { resolve_viable_source(ptr::from_mut(&mut resolution).cast(), visit, SOURCE_HOST) != 0 }
}

fn evaluate(value: &Residency, outputs: &Outputs, path: &bounds::Clause) -> Residency {
    let mut resolution = SourceResolution { outputs, active: BTreeSet::new() };
    let mut result = Residency::Absent;
    let visit = SourceVisit::new(value, path, &mut result);
    unsafe { resolve_residency_source(ptr::from_mut(&mut resolution).cast(), visit, SOURCE_HOST); }
    result
}

pub(super) fn output(
    value: &mut Returned,
    function: &str,
    kind: OutputKind,
    arguments: &mut Option<SharedArguments>,
    bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
) {
    visit(value, &mut Input::new(0), &mut |data, path| {
        let mut bindings=bindings.iter().map(|(id,proof)|(*id,proof.numeric_facts())).collect::<BTreeMap<_,_>>();
        for index in 0..path.depth(){let id=ValueIdentity::ElementIndex(index);bindings.insert(id,bounds::Proof::default().with_identity(id));}
        data.residency = Residency::Origins(BTreeSet::from([Origin::Output(Output {
                function: function.into(),
                kind:kind.clone(),
                fields: path.fields.clone(),
                arguments: Arc::clone(arguments.get_or_insert_with(Arc::default)),
                bindings,
            })]));
    });
}

#[derive(Default)]
struct OutputSnapshot {
    fields: Vec<Part>,
    paths: OutputPaths,
}

#[repr(u32)]
enum PathPart { Elements, Field }

impl PathPart {
    fn at(self, index: usize) -> Part {
        match self {
            Self::Elements => Part::Elements,
            Self::Field => Part::Field(index),
        }
    }
}

#[repr(u32)]
enum EmptyElements { Skip, Record }

#[repr(C)]
struct ResourcePathHost {
    dataset: unsafe extern "C" fn(Context, Context),
    maximum: unsafe extern "C" fn(Context) -> u32,
    element: unsafe extern "C" fn(Context) -> Context,
    field: unsafe extern "C" fn(Context, usize) -> Context,
    edge: unsafe extern "C" fn(Context, PathPart, usize, Context),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::walk_resource_paths"]
    fn walk_resource_paths(context: Context, node: AvailabilityNode, empty: EmptyElements, host: ResourcePathHost);
}

const SNAPSHOT_PATH_HOST: ResourcePathHost = ResourcePathHost {
    dataset: snapshot_dataset,
    maximum: dataset_maximum,
    element: snapshot_element,
    field: snapshot_field,
    edge: snapshot_edge,
};

// Snapshot addresses are borrowed. Only the snapshot host reads through them.
fn snapshot_shape(value: &Returned) -> AvailabilityNode {
    match value {
        Returned::Dataset(data) => AvailabilityNode {
            kind: AvailabilityKind::Dataset,
            model: ptr::from_ref(data).cast_mut().cast(),
            fields: 0,
        },
        Returned::OwnedValue(value) => AvailabilityNode {
            kind: AvailabilityKind::Aggregate,
            model: ptr::from_ref(value).cast_mut().cast(),
            fields: value.payloads.len(),
        },
        _ => AvailabilityNode { kind: AvailabilityKind::Value, model: ptr::null_mut(), fields: 0 },
    }
}

unsafe extern "C" fn snapshot_dataset(context: Context, model: Context) {
    let snapshot = unsafe { &mut *context.cast::<OutputSnapshot>() };
    let data = unsafe { &*model.cast::<application::Data>() };
    snapshot.paths.insert(snapshot.fields.clone(), data.residency.clone());
}

unsafe extern "C" fn snapshot_element(model: Context) -> Context {
    let data = unsafe { &*model.cast::<application::Data>() };
    data.elements.known().map_or(ptr::null_mut(), |element| ptr::from_ref(element).cast_mut().cast())
}

unsafe extern "C" fn snapshot_field(model: Context, index: usize) -> Context {
    let value = unsafe { &*model.cast::<owned_values::OwnedValue>() };
    value.payloads[index].as_deref().map_or(ptr::null_mut(), |value| ptr::from_ref(value).cast_mut().cast())
}

unsafe extern "C" fn snapshot_edge(context: Context, part: PathPart, index: usize, child: Context) {
    let snapshot = unsafe { &mut *context.cast::<OutputSnapshot>() };
    snapshot.fields.push(part.at(index));
    if let Some(value) = unsafe { child.cast::<Returned>().as_ref() } {
        unsafe { walk_resource_paths(ptr::from_mut(snapshot).cast(), snapshot_shape(value), EmptyElements::Record, SNAPSHOT_PATH_HOST) };
    } else {
        snapshot.paths.insert(snapshot.fields.clone(), Residency::Absent);
    }
    snapshot.fields.pop();
}

pub(super) fn output_sources<'a>(
    units: impl Iterator<Item = (&'a str, Option<&'a Returned>, &'a [Option<Returned>])>,
) -> Outputs {
    units.map(|(name, returned, written)| {
        let returned = returned.into_iter().map(|value| (OutputKind::Returned, value));
        let written = written.iter().enumerate().filter_map(|(index, value)| {
            value.as_ref().map(|value| (OutputKind::Written(index), value))
        });
        let values = returned.chain(written).filter_map(|(kind, value)| {
            let mut snapshot = OutputSnapshot::default();
            unsafe {
                walk_resource_paths(ptr::from_mut(&mut snapshot).cast(), snapshot_shape(value), EmptyElements::Record, SNAPSHOT_PATH_HOST);
            }
            (!snapshot.paths.is_empty()).then_some((kind, snapshot.paths))
        }).collect();
        (name.to_owned(), values)
    }).collect()
}

#[repr(u32)]
enum AvailabilityKind { Value, Dataset, Aggregate, Variant }

#[repr(C)]
struct AvailabilityNode {
    kind: AvailabilityKind,
    model: Context,
    fields: usize,
}

#[repr(C)]
struct OutputAvailabilityHost {
    inspect: unsafe extern "C" fn(Context) -> AvailabilityNode,
    present: unsafe extern "C" fn(Borrowed, Context) -> u32,
    maximum: unsafe extern "C" fn(Context) -> u32,
    element: unsafe extern "C" fn(Context) -> Context,
    field: unsafe extern "C" fn(Context, usize) -> Context,
    discard: unsafe extern "C" fn(Context, usize),
}

unsafe extern "C" {
    #[link_name = "bootstrap.ownership_path::resolve_output_availability"]
    fn resolve_output_availability(source: Borrowed, value: Context, host: OutputAvailabilityHost) -> u32;
}

unsafe extern "C" fn inspect_availability(value: Context) -> AvailabilityNode {
    match unsafe { &mut *value.cast::<Returned>() } {
        Returned::Dataset(data) => AvailabilityNode {
            kind: AvailabilityKind::Dataset,
            model: ptr::from_mut(data).cast(),
            fields: 0,
        },
        Returned::OwnedValue(value) => owned_availability(value),
        _ => AvailabilityNode { kind: AvailabilityKind::Value, model: ptr::null_mut(), fields: 0 },
    }
}

fn owned_availability(value: &mut owned_values::OwnedValue) -> AvailabilityNode {
    AvailabilityNode {
        kind: if value.value.ty.has_variants() { AvailabilityKind::Variant } else { AvailabilityKind::Aggregate },
        fields: value.payloads.len(),
        model: ptr::from_mut(value).cast(),
    }
}

unsafe extern "C" fn dataset_available(outputs: Borrowed, model: Context) -> u32 {
    let data = unsafe { &*model.cast::<application::Data>() };
    let outputs = unsafe { &*outputs.cast::<Outputs>() };
    u32::from(present(&data.residency, outputs))
}

unsafe extern "C" fn dataset_maximum(model: Context) -> u32 {
    unsafe { &*model.cast::<application::Data>() }.length.maximum()
}

unsafe extern "C" fn dataset_element(model: Context) -> Context {
    let data = unsafe { &mut *model.cast::<application::Data>() };
    data.elements.known_mut().map_or(ptr::null_mut(), |element| ptr::from_mut(element).cast())
}

unsafe extern "C" fn availability_field(model: Context, index: usize) -> Context {
    let value = unsafe { &mut *model.cast::<owned_values::OwnedValue>() };
    value.payloads[index].as_deref_mut().map_or(ptr::null_mut(), |value| ptr::from_mut(value).cast())
}

unsafe extern "C" fn discard_field(model: Context, index: usize) {
    let value = unsafe { &mut *model.cast::<owned_values::OwnedValue>() };
    value.payloads[index] = None;
}

const OUTPUT_AVAILABILITY_HOST: OutputAvailabilityHost = OutputAvailabilityHost {
    inspect: inspect_availability,
    present: dataset_available,
    maximum: dataset_maximum,
    element: dataset_element,
    field: availability_field,
    discard: discard_field,
};

pub(super) fn resolve_returned(value: &mut Option<Returned>, outputs: &Outputs) {
    let Some(model) = value.as_mut() else { return; };
    let available = unsafe {
        resolve_output_availability(ptr::from_ref(outputs).cast(), ptr::from_mut(model).cast(), OUTPUT_AVAILABILITY_HOST)
    };
    if available == 0 {
        *value = None;
    }
}



// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Part);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        field: ReadCallback,
        elements: ReadCallback,
        selected: ReadCallback,
    }
    unsafe extern "C" fn build_field(context: Slot, destination: Slot) -> ReadStatus {
        let mut field = <usize as Native>::empty();
        #[repr(C)]
        struct Fields {
            field: Slot,
        }
        #[repr(C)]
        struct Host {
            field: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_part_field"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    field: contracts::slot(&mut field),
                },
                Host {
                    field: contracts::read_value::<usize>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Part>() = Part::Field(field);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_elements(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Part>() = Part::Elements;
        }
        ReadStatus::Success
    }
    #[repr(C)]
    struct Selection { index: Slot }

    #[repr(C)]
    struct SelectionReader { index: ReadCallback }

    unsafe fn read_selection<T: Native>(context: Slot, destination: Slot, make: impl FnOnce(T) -> Part) -> ReadStatus {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_part_selected"]
            fn payload(context: Slot, fields: Selection, host: SelectionReader) -> ReadStatus;
        }
        let mut index = T::empty();
        let status = unsafe {
            payload(context, Selection { index: contracts::slot(&mut index) },
                SelectionReader { index: contracts::read_value::<T> })
        };
        if status == ReadStatus::Success {
            unsafe { *destination.cast::<Part>() = make(index); }
        }
        status
    }

    unsafe extern "C" fn build_selected(context: Slot, destination: Slot) -> ReadStatus {
        unsafe { read_selection(context, destination, Part::Selected) }
    }

    #[repr(C)]
    struct SelectionWriter { index: WriteCallback }

    unsafe fn write_selection<T: Native>(output: Slot, selection: &T) {
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_residency_part_selected"]
            fn write(output: Slot, fields: Selection, host: SelectionWriter,
                append: unsafe extern "C" fn(Slot, *const u8, usize));
        }
        unsafe {
            write(output, Selection { index: contracts::address(selection).cast_mut() },
                SelectionWriter { index: contracts::write_value::<T> }, contracts::append);
        }
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_residency_part"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Part {
        fn empty() -> Self {
            Self::Elements
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Field(field) => {
                    #[repr(C)]
                    struct Fields {
                        field: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        field: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_part_field"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                field: contracts::address(field).cast_mut(),
                            },
                            Host {
                                field: contracts::write_value::<usize>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Elements => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_part_elements"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Selected(index) => unsafe { write_selection(destination, index); },
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        field: build_field,
                        elements: build_elements,
                        selected: build_selected,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Input);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        parameter: Slot,
        fields: Slot,
    }
    #[repr(C)]
    struct Writer {
        parameter: WriteCallback,
        fields: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        parameter: ReadCallback,
        fields: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_residency_input"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_residency_input"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Input {
        fn empty() -> Self {
            Self {
                parameter: <usize as Native>::empty(),
                fields: <Vec<Part> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        parameter: contracts::address(&self.parameter).cast_mut(),
                        fields: contracts::address(&self.fields).cast_mut(),
                    },
                    Writer {
                        parameter: contracts::write_value::<usize>,
                        fields: contracts::write_value::<Vec<Part>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        parameter: contracts::slot(&mut output.parameter),
                        fields: contracts::slot(&mut output.fields),
                    },
                    Reader {
                        parameter: contracts::read_value::<usize>,
                        fields: contracts::read_value::<Vec<Part>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Origin);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        cpu: ReadCallback,
        gpu: ReadCallback,
        input: ReadCallback,
        output: ReadCallback,
        task_transfer: ReadCallback,
    }
    unsafe extern "C" fn build_cpu(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Origin>() = Origin::Cpu;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_gpu(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Origin>() = Origin::Gpu;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_input(context: Slot, destination: Slot) -> ReadStatus {
        let mut input = <Input as Native>::empty();
        #[repr(C)]
        struct Fields {
            input: Slot,
        }
        #[repr(C)]
        struct Host {
            input: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_origin_input"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    input: contracts::slot(&mut input),
                },
                Host {
                    input: contracts::read_value::<Input>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Origin>() = Origin::Input(input);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_output(context: Slot, destination: Slot) -> ReadStatus {
        let mut output = <Output as Native>::empty();
        #[repr(C)]
        struct Fields {
            output: Slot,
        }
        #[repr(C)]
        struct Host {
            output: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_origin_output"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    output: contracts::slot(&mut output),
                },
                Host {
                    output: contracts::read_value::<Output>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Origin>() = Origin::Output(output);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_task_transfer(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Origin>() = Origin::TaskTransfer;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_residency_origin"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Origin {
        fn empty() -> Self {
            Self::Cpu
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Cpu => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_cpu"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Gpu => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_gpu"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Input(input) => {
                    #[repr(C)]
                    struct Fields {
                        input: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        input: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_input"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                input: contracts::address(input).cast_mut(),
                            },
                            Host {
                                input: contracts::write_value::<Input>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Output(output) => {
                    #[repr(C)]
                    struct Fields {
                        output: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        output: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_output"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                output: contracts::address(output).cast_mut(),
                            },
                            Host {
                                output: contracts::write_value::<Output>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::TaskTransfer => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origin_task_transfer"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        cpu: build_cpu,
                        gpu: build_gpu,
                        input: build_input,
                        output: build_output,
                        task_transfer: build_task_transfer,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Residency);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        absent: ReadCallback,
        origins: ReadCallback,
        selected: ReadCallback,
    }
    unsafe extern "C" fn build_absent(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Residency>() = Residency::Absent;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_origins(context: Slot, destination: Slot) -> ReadStatus {
        let mut origins = <BTreeSet<Origin> as Native>::empty();
        #[repr(C)]
        struct Fields {
            origins: Slot,
        }
        #[repr(C)]
        struct Host {
            origins: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_origins"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    origins: contracts::slot(&mut origins),
                },
                Host {
                    origins: contracts::read_value::<BTreeSet<Origin>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Residency>() = Residency::Origins(origins);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_selected(context: Slot, destination: Slot) -> ReadStatus {
        let mut when = <Box<bounds::Condition> as Native>::empty();
        let mut a = <Box<Residency> as Native>::empty();
        let mut b = <Box<Residency> as Native>::empty();
        #[repr(C)]
        struct Fields {
            when: Slot,
            a: Slot,
            b: Slot,
        }
        #[repr(C)]
        struct Host {
            when: ReadCallback,
            a: ReadCallback,
            b: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_selected"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    when: contracts::slot(&mut when),
                    a: contracts::slot(&mut a),
                    b: contracts::slot(&mut b),
                },
                Host {
                    when: contracts::read_value::<Box<bounds::Condition>>,
                    a: contracts::read_value::<Box<Residency>>,
                    b: contracts::read_value::<Box<Residency>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Residency>() = Residency::Selected(when, a, b);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_residency"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Residency {
        fn empty() -> Self {
            Self::Absent
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Absent => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_absent"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Origins(origins) => {
                    #[repr(C)]
                    struct Fields {
                        origins: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        origins: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_origins"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                origins: contracts::address(origins).cast_mut(),
                            },
                            Host {
                                origins: contracts::write_value::<BTreeSet<Origin>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Selected(when, a, b) => {
                    #[repr(C)]
                    struct Fields {
                        when: Slot,
                        a: Slot,
                        b: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        when: WriteCallback,
                        a: WriteCallback,
                        b: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_selected"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                when: contracts::address(when).cast_mut(),
                                a: contracts::address(a).cast_mut(),
                                b: contracts::address(b).cast_mut(),
                            },
                            Host {
                                when: contracts::write_value::<Box<bounds::Condition>>,
                                a: contracts::write_value::<Box<Residency>>,
                                b: contracts::write_value::<Box<Residency>>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        absent: build_absent,
                        origins: build_origins,
                        selected: build_selected,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(OutputKind);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        returned: ReadCallback,
        written: ReadCallback,
    }
    unsafe extern "C" fn build_returned(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<OutputKind>() = OutputKind::Returned;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_written(context: Slot, destination: Slot) -> ReadStatus {
        let mut parameter = <usize as Native>::empty();
        #[repr(C)]
        struct Fields {
            parameter: Slot,
        }
        #[repr(C)]
        struct Host {
            parameter: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_residency_output_kind_written"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    parameter: contracts::slot(&mut parameter),
                },
                Host {
                    parameter: contracts::read_value::<usize>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<OutputKind>() = OutputKind::Written(parameter);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_residency_output_kind"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for OutputKind {
        fn empty() -> Self {
            Self::Returned
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Returned => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_output_kind_returned"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Written(parameter) => {
                    #[repr(C)]
                    struct Fields {
                        parameter: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        parameter: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_residency_output_kind_written"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                parameter: contracts::address(parameter).cast_mut(),
                            },
                            Host {
                                parameter: contracts::write_value::<usize>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        returned: build_returned,
                        written: build_written,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Output);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        function: Slot,
        kind: Slot,
        fields: Slot,
        arguments: Slot,
        bindings: Slot,
    }
    #[repr(C)]
    struct Writer {
        function: WriteCallback,
        kind: WriteCallback,
        fields: WriteCallback,
        arguments: WriteCallback,
        bindings: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        function: ReadCallback,
        kind: ReadCallback,
        fields: ReadCallback,
        arguments: ReadCallback,
        bindings: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_residency_output"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_residency_output"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Output {
        fn empty() -> Self {
            Self {
                function: <String as Native>::empty(),
                kind: <OutputKind as Native>::empty(),
                fields: <Vec<Part> as Native>::empty(),
                arguments: <SharedArguments as Native>::empty(),
                bindings: <BTreeMap<ValueIdentity, bounds::Proof> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        function: contracts::address(&self.function).cast_mut(),
                        kind: contracts::address(&self.kind).cast_mut(),
                        fields: contracts::address(&self.fields).cast_mut(),
                        arguments: contracts::address(&self.arguments).cast_mut(),
                        bindings: contracts::address(&self.bindings).cast_mut(),
                    },
                    Writer {
                        function: contracts::write_value::<String>,
                        kind: contracts::write_value::<OutputKind>,
                        fields: contracts::write_value::<Vec<Part>>,
                        arguments: contracts::write_value::<SharedArguments>,
                        bindings: contracts::write_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        function: contracts::slot(&mut output.function),
                        kind: contracts::slot(&mut output.kind),
                        fields: contracts::slot(&mut output.fields),
                        arguments: contracts::slot(&mut output.arguments),
                        bindings: contracts::slot(&mut output.bindings),
                    },
                    Reader {
                        function: contracts::read_value::<String>,
                        kind: contracts::read_value::<OutputKind>,
                        fields: contracts::read_value::<Vec<Part>>,
                        arguments: contracts::read_value::<SharedArguments>,
                        bindings: contracts::read_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Contract);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Address, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        required: Slot,
        calls: Slot,
    }
    #[repr(C)]
    struct Writer {
        required: WriteCallback,
        calls: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        required: ReadCallback,
        calls: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_residency_contract"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_residency_contract"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    unsafe extern "C" fn write_required(value: Address, output: Slot) {
        unsafe {
            contracts::write_map_with(&*value.cast::<Requirements>(), output, write_requirement)
        }
    }
    unsafe extern "C" fn write_requirement(value: Address, output: Slot) {
        let (key, guards) =
            unsafe { &*value.cast::<(&(Input, Target), &BTreeSet<bounds::Condition>)>() };
        let condition = guards
            .iter()
            .cloned()
            .fold(bounds::Condition::constant(false), |a, b| {
                a.logical(b, true)
            });
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_write_requirement"]
            fn write(
                key: Address,
                condition: Address,
                output: Slot,
                key_write: WriteCallback,
                condition_write: WriteCallback,
                append: unsafe extern "C" fn(Slot, *const u8, usize),
            );
        }
        unsafe {
            write(
                contracts::address(*key),
                contracts::address(&condition),
                output,
                contracts::write_value::<(Input, Target)>,
                contracts::write_value::<bounds::Condition>,
                contracts::append,
            )
        }
    }
    impl Native for Contract {
        fn empty() -> Self {
            Self::default()
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        required: contracts::address(&self.required).cast_mut(),
                        calls: contracts::address(&self.calls).cast_mut(),
                    },
                    Writer {
                        required: write_required,
                        calls: contracts::write_value::<Vec<Call>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut required = <Requirements as Native>::empty();
            let mut calls = <Vec<Call> as Native>::empty();
            let status = unsafe {
                read(
                    context,
                    Fields {
                        required: contracts::slot(&mut required),
                        calls: contracts::slot(&mut calls),
                    },
                    Reader {
                        required: contracts::read_value::<Requirements>,
                        calls: contracts::read_value::<Vec<Call>>,
                    },
                )
            };
            if status == ReadStatus::Success {
                *output = Self { required, calls };
            }
            status
        }
    }
};

crate::codec::contracts::codec!(Call);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        function: Slot,
        arguments: Slot,
        bindings: Slot,
        path: Slot,
    }
    #[repr(C)]
    struct Writer {
        function: WriteCallback,
        arguments: WriteCallback,
        bindings: WriteCallback,
        path: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        function: ReadCallback,
        arguments: ReadCallback,
        bindings: ReadCallback,
        path: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_residency_call"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_residency_call"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Call {
        fn empty() -> Self {
            Self {
                function: String::new(),
                arguments: <SharedArguments as Native>::empty(),
                bindings: BTreeMap::new(),
                path: bounds::Clause::default(),
                source: None,
                span: Span::default(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            let path = bounds::Condition::guard(self.path.clone());
            unsafe {
                write(
                    output,
                    Fields {
                        function: contracts::address(&self.function).cast_mut(),
                        arguments: contracts::address(&self.arguments).cast_mut(),
                        bindings: contracts::address(&self.bindings).cast_mut(),
                        path: contracts::address(&path).cast_mut(),
                    },
                    Writer {
                        function: contracts::write_value::<String>,
                        arguments: contracts::write_value::<SharedArguments>,
                        bindings: contracts::write_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                        path: contracts::write_value::<bounds::Condition>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut function = <String as Native>::empty();
            let mut arguments = <SharedArguments as Native>::empty();
            let mut bindings = <BTreeMap<ValueIdentity, bounds::Proof> as Native>::empty();
            let mut path = <bounds::Condition as Native>::empty();
            let status = unsafe {
                read(
                    context,
                    Fields {
                        function: contracts::slot(&mut function),
                        arguments: contracts::slot(&mut arguments),
                        bindings: contracts::slot(&mut bindings),
                        path: contracts::slot(&mut path),
                    },
                    Reader {
                        function: contracts::read_value::<String>,
                        arguments: contracts::read_value::<SharedArguments>,
                        bindings: contracts::read_value::<BTreeMap<ValueIdentity, bounds::Proof>>,
                        path: contracts::read_value::<bounds::Condition>,
                    },
                )
            };
            if status == ReadStatus::Success {
                *output = Self {
                    function,
                    arguments,
                    bindings,
                    path: path.assume(&bounds::Clause::default(), true),
                    source: None,
                    span: Span::default(),
                };
            }
            status
        }
    }
};
