use super::*;
use std::borrow::Cow;
use crate::module::codec::{Encode,Decode};

#[derive(Clone)]
pub(in crate::check) enum Epoch { Input, Local { memory:usize, snapshots:Option<Arc<[ValueSnapshot]>> } }
impl Epoch { fn key(&self)->Option<usize>{match self{Self::Input=>None,Self::Local{memory,..}=>Some(*memory)}} }
impl PartialEq for Epoch{fn eq(&self,b:&Self)->bool{self.key()==b.key()}}
impl Eq for Epoch{}
impl PartialOrd for Epoch{fn partial_cmp(&self,b:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(b))}}
impl Ord for Epoch{fn cmp(&self,b:&Self)->std::cmp::Ordering{self.key().cmp(&b.key())}}
impl Encode for Epoch{fn encode(&self,_:&mut crate::module::codec::Writer){}}
impl Decode for Epoch{fn decode(_:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{Ok(Self::Input)}}
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
enum Op { Identity, Position(address::Origin), Initial{origin:address::Origin,frame:Option<usize>}, Read{word:u32,epoch:Epoch,frame:Option<usize>}, Element{fields:Vec<usize>,index:Option<bounds::Operand>}, Projection(Vec<usize>), Resources, Owners, Select(Vec<Option<bounds::Operand>>) }
crate::module::codec::variants!(Op {0=>Identity,1=>Position(origin),2=>Read{word,epoch,frame},3=>Element{fields,index},4=>Projection(fields),5=>Resources,6=>Owners,7=>Select(indices),8=>Initial{origin,frame}});
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Edge { from:usize, op:Op, when:bounds::Condition }
crate::module::codec::record!(Edge{from,op,when});
#[derive(Clone,Default,PartialEq,Eq,PartialOrd,Ord)]
struct Node { seeds:Borrow, edges:Vec<Edge> }
crate::module::codec::record!(Node{seeds,edges});
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Write { target:usize, fields:Option<Vec<(u32,usize)>>, when:bounds::Condition }
crate::module::codec::record!(Write{target,fields,when});
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Frame { parent:Option<usize>, writes:Vec<Write> }
crate::module::codec::record!(Frame{parent,writes});
#[derive(Clone,Default,PartialEq,Eq,PartialOrd,Ord)]
pub(in crate::check) struct Graph { nodes:Vec<Node>, variables:BTreeSet<usize>, frames:Vec<Frame>, inputs:BTreeMap<LoopInput,usize> }
impl Encode for Graph{fn encode(&self,w:&mut crate::module::codec::Writer){self.nodes.encode(w);self.variables.encode(w);self.frames.encode(w);}}
impl Decode for Graph{fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{
    let nodes=Vec::<Node>::decode(r)?;let variables=BTreeSet::<usize>::decode(r)?;let frames=Vec::<Frame>::decode(r)?;
    if nodes.is_empty()||variables.iter().any(|id|*id>=nodes.len())||nodes.iter().any(|n|n.edges.iter().any(|e|e.from>=nodes.len()||matches!(&e.op,Op::Read{frame:Some(f),..}|Op::Initial{frame:Some(f),..}if *f>=frames.len())))||frames.iter().enumerate().any(|(i,f)|f.parent.is_some_and(|p|p>=i)||f.writes.iter().any(|w|w.target>=nodes.len()||w.fields.iter().flatten().any(|(_,v)|*v>=nodes.len()))){return Err("invalid bound read relation".into());}
    let graph=Self{nodes,variables,frames,inputs:BTreeMap::new()};
    if graph.order(&(0..graph.nodes.len()).collect()).len()!=graph.nodes.len()-graph.variables.len(){return Err("cyclic read expression without an input binding".into());}
    Ok(graph)
}}
#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
enum Key { Loop(LoopInput), Read(ContentRead), Call(CallResult) }
struct Builder<'a> { function:&'a Function,state:&'a State<'a>,known:&'a States<'a>,keys:BTreeMap<Key,usize>,prefix:Vec<Option<usize>>,graph:Graph,parameters:BTreeMap<ValueIdentity,usize> }
impl Builder<'_> {
    fn empty(&mut self)->usize{let id=self.graph.nodes.len();self.graph.nodes.push(Node::default());id}
    fn edge(&mut self,to:usize,from:usize,op:Op,when:bounds::Clause){self.graph.nodes[to].edges.push(Edge{from,op,when:bounds::Condition::guard(when)})}
    fn borrow(&mut self,value:&Borrow,epoch:&Epoch)->usize{let id=self.empty();self.fill(id,value,epoch);id}
    fn read(&mut self,to:usize,from:usize,word:u32,epoch:Epoch){let frame=match &epoch{Epoch::Input=>None,Epoch::Local{memory,..}=>self.read_frame(*memory)};self.edge(to,from,Op::Read{word,epoch,frame},bounds::Clause::default());}
    fn fill(&mut self,id:usize,value:&Borrow,epoch:&Epoch){
        for(source,when)in value.sources.entries(){
            let target=match source {
                Origin::Parameter(input,indices)if self.parameters.contains_key(input)=>{let from=self.parameters[input];let n=self.empty();self.edge(n,from,Op::Select(indices.clone()),bounds::Clause::default());Some(n)},
                Origin::Position(origin)=>Some(self.position(origin,epoch)),
                Origin::Contents{origin,word}=>{let from=self.position(origin,epoch);let n=self.empty();self.read(n,from,*word,epoch.clone());Some(n)},
                Origin::Element{parent,fields,index}=>{let from=self.borrow(parent,epoch);let n=self.empty();self.edge(n,from,Op::Element{fields:fields.clone(),index:*index},bounds::Clause::default());Some(n)},
                Origin::Projection{parent,fields}=>{let from=self.borrow(parent,epoch);let n=self.empty();self.edge(n,from,Op::Projection(fields.clone()),bounds::Clause::default());Some(n)},
                Origin::Relation(graph,root)=>Some(self.import(graph,*root,None,&Epoch::Input)),
                _=>None,
            };
            if let Some(from)=target{self.edge(id,from,Op::Identity,when);}else{self.graph.nodes[id].seeds.sources.insert_when(source.clone(),when);}
        }
        for(input,when)in value.inputs.entries(){let from=self.key(Key::Loop(*input));self.edge(id,from,Op::Identity,when);}
        for(read,when)in value.reads.entries(){let from=self.key(Key::Read(read.clone()));self.edge(id,from,Op::Identity,when);}
        for(call,when)in value.calls.entries(){let from=self.key(Key::Call(call.clone()));self.edge(id,from,Op::Identity,when);}
    }
    fn position(&mut self,value:&address::Origin,epoch:&Epoch)->usize{
        if let address::Root::Parameter(parameter)=&value.root && let Some(&from)=self.parameters.get(&ValueIdentity::ParameterStorage(*parameter)){
            let id=self.empty();let mut template=value.clone();template.root=address::Root::Derived(Borrow::default());template.positions=Arc::from([]);self.edge(id,from,Op::Position(template),bounds::Clause::default());return id;
        }
        let parent=match &value.root{address::Root::Derived(b)|address::Root::Resource{borrow:b,..}|address::Root::Dataset{borrow:b,..}|address::Root::External(address::Identity::Storage(b))=>Some(b),_=>None};
        let id=self.empty();
        if let Some(parent)=parent{let from=self.borrow(parent,epoch);let mut template=value.map_borrows(|_|Borrow::default());template.positions=Arc::from([]);self.edge(id,from,Op::Position(template),bounds::Clause::default());}
        else{self.graph.nodes[id].seeds.sources.insert(Origin::Position(Arc::new(value.clone())));}
        id
    }
    fn writes(&mut self, memory: usize) -> Vec<Write> {
        let usage = &self.function.uses[memory];
        let Requirement::Memory { borrow, access, written, address_fields, .. } = &usage.requirement else {
            return Vec::new();
        };
        let mut writes = Vec::new();
        if let Some(call) = self.function.calls.iter().find(|call| call.memory == memory) {
            for access in &self.known[&call.function].interface.access {
                if let Access::Stored { borrows, word, value, when } = access {
                    let target = self.borrow(&call_content_sources(borrows, call, &call.arguments).at_call(&call.arguments), &Epoch::Input);
                    let value = self.borrow(&call_content_sources(value, call, &call.arguments).at_call(&call.arguments), &Epoch::Input);
                    writes.push(Write {
                        target,
                        fields: Some(vec![(*word, value)]),
                        when: when.at_call(&call.arguments),
                    });
                }
            }
        } else if access.is_some_and(super::super::Access::writes) {
            let target = self.borrow(borrow, &Epoch::Input);
            let fields = written.as_ref().map(|value| {
                address_fields.iter().map(|&(word, component)| {
                    (word, self.borrow(&value.view_borrow(component), &Epoch::Input))
                }).collect()
            });
            writes.push(Write { target, fields, when: bounds::Condition::constant(true) });
        }
        writes
    }

    fn frame(&mut self, memory: usize) -> Option<usize> {
        while self.prefix.len() <= memory {
            let at = self.prefix.len() - 1;
            let parent = *self.prefix.last().unwrap();
            self.prefix.push(parent);
            let mut writes = self.writes(at);
            if writes.is_empty() { continue; }
            let execution = bounds::Condition::execution(&self.function.uses[at].path);
            for write in &mut writes {
                write.when = write.when.clone().logical(execution.clone(), false);
            }
            let id = self.graph.frames.len();
            self.graph.frames.push(Frame { parent, writes });
            self.prefix[at + 1] = Some(id);
        }
        self.prefix[memory]
    }
    fn read_frame(&mut self,memory:usize)->Option<usize>{
        let mut at=self.frame(memory);let mut frames=Vec::new();while let Some(id)=at{frames.push(id);at=self.graph.frames[id].parent;}
        let path=&self.function.uses[memory].path;let mut parent=None;let mut normal=bounds::Normalizer::new();
        for id in frames.into_iter().rev(){
            let writes=self.graph.frames[id].writes.iter().map(|w|Write{target:w.target,fields:w.fields.clone(),when:normal.condition(&bounds::Condition::paths(w.when.assume(path,true),w.when.assume(path,false)))}).collect();
            let frame=Frame{parent,writes};let id=match self.graph.frames.iter().position(|f|*f==frame){Some(id)=>id,None=>{let id=self.graph.frames.len();self.graph.frames.push(frame);id}};parent=Some(id);
        }
        if let Some(observe) = self.known.observe {
            observe(self.state.name, crate::compiler::BorrowSolveStorage::ReadFrame {
                memory, tables: normal.storage(),
            });
        }
        parent
    }
    fn key(&mut self,key:Key)->usize{
        if let Some(&id)=self.keys.get(&key){return id;}
        let id=self.empty();self.keys.insert(key.clone(),id);
        match key{
            Key::Loop(input)=>{self.graph.inputs.insert(input,id);self.graph.variables.insert(id);self.fill(id,&self.function.loops[&input].borrow.clone(),&Epoch::Input)},
            Key::Read(ContentRead::Relation { memory, graph, root, snapshots }) => {
                let prefix = self.read_frame(memory);
                let from = self.import(&graph, root, prefix, &Epoch::Local { memory, snapshots });
                self.edge(id, from, Op::Identity, bounds::Clause::default());
            },
            Key::Read(ContentRead::Value{memory,word,borrow,snapshots})=>{let from=self.borrow(&borrow,&Epoch::Input);self.read(id,from,word,Epoch::Local{memory,snapshots:Some(snapshots)});self.graph.nodes[id].edges.last_mut().unwrap().when=bounds::Condition::execution(&self.function.uses[memory].path);},
            Key::Read(ContentRead::Call{memory,origin,word,snapshots})=>{let from=self.position(&origin,&Epoch::Input);self.read(id,from,word,Epoch::Local{memory,snapshots});self.graph.nodes[id].edges.last_mut().unwrap().when=bounds::Condition::execution(&self.function.uses[memory].path);},
            Key::Read(ContentRead::Snapshot{memory,position:Position::Local(slot),initial,..})=>{
                let from=self.borrow(&initial.view_borrow(0),&Epoch::Input);
                let writes = self.writes(memory);
                let frame = if writes.is_empty() {
                    None
                } else {
                    let id = self.graph.frames.len();
                    self.graph.frames.push(Frame { parent: None, writes });
                    Some(id)
                };
                let layout=address::Layout::of(&Ty::scalar(Scalar::U64));
                let origin=address::Origin{root:address::Root::Local{base:slot,len:1},layout:layout.clone(),base:layout,words:Arc::from([0]),positions:Arc::from([Position::Local(slot)]),writable:true,initialized:Arc::from([true])};
                let path = self.function.uses[memory].path.clone();
                self.graph.nodes[id].edges.push(Edge {
                    from,
                    op: Op::Initial { origin, frame },
                    when: bounds::Condition::paths(path, bounds::Clause::impossible()),
                });
            },
            Key::Read(read)=>{
                let epoch=match &read{ContentRead::Snapshot{memory,..}|ContentRead::Returned{memory}=>Epoch::Local{memory:*memory,snapshots:None},_=>Epoch::Input};
                let mut evaluation=BorrowEvaluation{function:self.function,state:self.state,known:self.known,expand_loops:false,deferred_reads:true,loop_bindings:Vec::new(),values:BTreeMap::new()};let value=evaluation.content_read(&read,LoopSources::Deferred);self.fill(id,&value,&epoch);
            },
            Key::Call(result)=>{let call=&self.function.calls[result.call];let value=self.known[&call.function].interface.outputs.get(&result.output).map(|output|call_content_sources(output,call,&call.arguments).at_call(&call.arguments)).unwrap_or_else(||Borrow::source(Origin::Unresolved));self.fill(id,&value,&Epoch::Local{memory:call.memory,snapshots:call.snapshots.clone()});},
        }id
    }
    fn import(&mut self, graph: &Graph, root: usize, prefix: Option<usize>, entry: &Epoch) -> usize {
        let offset=self.graph.nodes.len();self.graph.nodes.resize_with(offset+graph.nodes.len(),Node::default);let frame_offset=self.graph.frames.len();
        self.graph.frames.extend(graph.frames.iter().map(|f|Frame{parent:f.parent.map(|p|frame_offset+p).or(prefix),writes:f.writes.iter().map(|w|Write{target:offset+w.target,fields:w.fields.as_ref().map(|fields|fields.iter().map(|(word,id)|(*word,offset+id)).collect()),when:w.when.clone()}).collect()}));
        if matches!(entry, Epoch::Input) {
            for (input, id) in &graph.inputs {
                self.graph.inputs.entry(*input).or_insert(offset + id);
            }
        }
        for(i,node)in graph.nodes.iter().enumerate(){
            if graph.variables.contains(&i){self.graph.variables.insert(offset+i);}
            self.fill(offset+i,&node.seeds,&Epoch::Input);
            self.graph.nodes[offset + i].edges.extend(node.edges.iter().map(|edge| {
                let mut op = edge.op.clone();
                match &mut op {
                    Op::Read { epoch, frame, .. } => {
                        *frame = frame.map(|f| frame_offset + f).or(prefix);
                        if let Epoch::Local { .. } = entry {
                            *epoch = entry.clone();
                        }
                    }
                    Op::Initial { frame, .. } => {
                        *frame = frame.map(|f| frame_offset + f).or(prefix);
                    }
                    _ => {}
                }
                Edge { from: offset + edge.from, op, when: edge.when.clone() }
            }));
        }offset+root
    }
}
impl Graph {
    fn builder<'a>(function:&'a Function,state:&'a State<'a>,known:&'a States<'a>)->Builder<'a>{Builder{function,state,known,keys:BTreeMap::new(),prefix:vec![None],graph:Self::default(),parameters:BTreeMap::new()}}
    pub(super) fn recursive_inputs(name:&str,function:&Function,state:&State,known:&States)->Option<BTreeMap<ValueIdentity,Borrow>>{
        if !function.calls.iter().any(|call|call.function==name){return None;}
        let mut b=Self::builder(function,state,known);let mut roots=BTreeMap::new();
        for(&local,&formal)in &function.parameters{if function.input(formal){
            let id=*roots.entry(formal).or_insert_with(||{let id=b.empty();b.graph.variables.insert(id);b.graph.nodes[id].seeds=Borrow::source(Origin::Parameter(local,Vec::new()));id});
            b.parameters.insert(local,id);
        }}
        for call in function.calls.iter().filter(|call|call.function==name){for(input,value)in &call.arguments{if let Some(&id)=roots.get(input){
            b.fill(id,&value.view_borrow(0).restricted(&call.path),&Epoch::Local{memory:call.memory,snapshots:call.snapshots.clone()});
        }}}
        if !roots.values().any(|root|b.graph.read_feedback(*root)){return None;}
        let graph=Arc::new(b.graph);let mut evaluation=BorrowEvaluation{function,state,known,expand_loops:true,deferred_reads:false,loop_bindings:Vec::new(),values:BTreeMap::new()};
        Some(roots.into_iter().map(|(input,root)|{let value=graph.solve(root,&mut evaluation);(input,function.address_borrows(value).unwrap_or_default())}).collect())
    }
    pub(super) fn loops(
        inputs: &BTreeSet<LoopInput>, function: &Function, state: &State, known: &States,
    ) -> Option<Arc<Self>> {
        if inputs.is_empty() {
            return None;
        }
        let mut builder = Self::builder(function, state, known);
        for &input in inputs {
            builder.key(Key::Loop(input));
        }
        builder.graph.remove_identity_feedback();
        Some(Arc::new(builder.graph))
    }

    pub(super) fn input_root(&self, input: LoopInput) -> usize {
        self.inputs[&input]
    }
    pub(super) fn bind(&self,root:usize,function:&Function,state:&State,known:&States)->(Arc<Self>,usize){let mut b=Self::builder(function,state,known);let root=b.import(self,root,None,&Epoch::Input);(Arc::new(b.graph),root)}
    pub(super) fn bind_incoming(
        &self, root: usize, memory: usize, snapshots: Option<Arc<[ValueSnapshot]>>,
        function: &Function, state: &State, known: &States,
    ) -> (Arc<Self>, usize) {
        let mut builder = Self::builder(function, state, known);
        let prefix = builder.read_frame(memory);
        let root = builder.import(self, root, prefix, &Epoch::Local { memory, snapshots });
        (Arc::new(builder.graph), root)
    }
    fn remove_identity_feedback(&mut self) {
        for &variable in &self.variables {
            let mut edges = std::mem::take(&mut self.nodes[variable].edges);
            edges.retain(|edge| {
                let mut edge = edge;
                while edge.op == Op::Identity {
                    if edge.from == variable {
                        return false;
                    }
                    if self.variables.contains(&edge.from) {
                        break;
                    }
                    let next = &self.nodes[edge.from];
                    let [next_edge] = next.edges.as_slice() else { break; };
                    if !next.seeds.empty() {
                        break;
                    }
                    edge = next_edge;
                }
                true
            });
            self.nodes[variable].edges = edges;
        }
    }

    fn frames_at(&self,id:usize)->BTreeSet<usize>{let mut found=BTreeSet::new();for edge in &self.nodes[id].edges{if let Op::Read{frame,..}|Op::Initial{frame,..}=edge.op{let mut at=frame;while let Some(id)=at{if !found.insert(id){break;}at=self.frames[id].parent;}}}found}
    fn read_feedback(&self,root:usize)->bool{
        let mut todo=vec![(root,false)];let mut seen=BTreeSet::new();
        while let Some((id,read))=todo.pop(){
            if id==root&&read{return true;}
            if !seen.insert((id,read)){continue;}
            for edge in &self.nodes[id].edges{todo.push((edge.from,read||matches!(edge.op,Op::Read{..})));}
            let read=read||self.nodes[id].edges.iter().any(|edge|matches!(edge.op,Op::Read{..}));
            for f in self.frames_at(id){for w in &self.frames[f].writes{todo.push((w.target,read));todo.extend(w.fields.iter().flatten().map(|(_,v)|(*v,read)));}}
        }false
    }
    fn reachable(&self,root:usize)->BTreeSet<usize>{let mut todo=vec![root];let mut found=BTreeSet::new();while let Some(id)=todo.pop(){if found.insert(id){todo.extend(self.nodes[id].edges.iter().map(|e|e.from));for f in self.frames_at(id){for w in &self.frames[f].writes{todo.push(w.target);todo.extend(w.fields.iter().flatten().map(|(_,v)|*v));}}}}found}
    fn order(&self, reachable: &BTreeSet<usize>) -> Vec<(usize, bool)> {
        let mut counts = BTreeMap::new();
        let mut users: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &id in reachable.difference(&self.variables) {
            let edges = &self.nodes[id].edges;
            let mut dependencies = edges.iter().map(|edge| edge.from).collect::<BTreeSet<_>>();
            for frame in self.frames_at(id) {
                for write in &self.frames[frame].writes {
                    dependencies.insert(write.target);
                    dependencies.extend(write.fields.iter().flatten().map(|(_, id)| *id));
                }
            }

            // A read can also use the evaluation scope's current loop bindings.
            let mut feedback = edges.iter().any(|edge| matches!(edge.op, Op::Read { .. }));
            dependencies.retain(|dependency| {
                let variable = self.variables.contains(dependency);
                feedback |= variable;
                !variable
            });
            counts.insert(id, (dependencies.len(), feedback));
            for dependency in dependencies {
                users.entry(dependency).or_default().push(id);
            }
        }

        let mut ready = counts.iter().filter_map(|(&id, &(count, _))| {
            (count == 0).then_some(id)
        }).collect::<Vec<_>>();
        let mut order = Vec::new();
        while let Some(id) = ready.pop() {
            let feedback = counts[&id].1;
            order.push((id, feedback));
            for &user in users.get(&id).into_iter().flatten() {
                let (count, changes) = counts.get_mut(&user).unwrap();
                *changes |= feedback;
                *count -= 1;
                if *count == 0 {
                    ready.push(user);
                }
            }
        }
        order
    }
    pub(in crate::check) fn selected(&self,root:usize)->Self{
        let reach=self.reachable(root);let order=std::iter::once(root).chain(reach.iter().copied().filter(|id|*id!=root)).collect::<Vec<_>>();let indices=order.iter().enumerate().map(|(i,id)|(*id,i)).collect::<BTreeMap<_,_>>();let frames=reach.iter().flat_map(|id|self.frames_at(*id)).collect::<BTreeSet<_>>();let fi=frames.iter().enumerate().map(|(i,id)|(*id,i)).collect::<BTreeMap<_,_>>();
        let nodes=order.iter().map(|&id|Node{seeds:self.nodes[id].seeds.clone(),edges:self.nodes[id].edges.iter().map(|edge|{let mut op=edge.op.clone();if let Op::Read{frame,..}|Op::Initial{frame,..}=&mut op{*frame=frame.map(|id|fi[&id]);}Edge{from:indices[&edge.from],op,when:edge.when.clone()}}).collect()}).collect();
        let variables=self.variables.iter().filter_map(|id|indices.get(id).copied()).collect();let frames=frames.iter().map(|id|{let f=&self.frames[*id];Frame{parent:f.parent.map(|p|fi[&p]),writes:f.writes.iter().map(|w|Write{target:indices[&w.target],fields:w.fields.as_ref().map(|v|v.iter().map(|(word,id)|(*word,indices[id])).collect()),when:w.when.clone()}).collect()}}).collect();Self{nodes,variables,frames,inputs:BTreeMap::new()}
    }
    pub(in crate::check) fn has_local(&self,root:usize)->bool{self.reachable(root).iter().any(|&id|self.nodes[id].seeds.has_local())}
    pub(in crate::check) fn unresolved(&self,root:usize)->bool{self.reachable(root).iter().any(|&id|self.nodes[id].seeds.unresolved())}
    pub(in crate::check) fn symbolic(&self,root:usize)->bool{self.reachable(root).iter().any(|&id|self.nodes[id].seeds.sources.iter().any(|s|matches!(s,Origin::Parameter(..)|Origin::Contents{..}|Origin::Relation(..))))}
    pub(in crate::check) fn content_inputs(&self,root:usize)->bool{self.reachable(root).iter().any(|&id|self.nodes[id].edges.iter().any(|e|matches!(e.op,Op::Read{..}))||self.nodes[id].seeds.content_inputs())}
    pub(in crate::check) fn pending(&self,root:usize)->bool{self.reachable(root).iter().map(|id|&self.nodes[*id]).any(|n|n.seeds.pending()||n.seeds.sources.iter().any(|s|matches!(s,Origin::Relation(..)|Origin::Element{..}|Origin::Projection{..})||matches!(s,Origin::Position(p)if matches!(p.root,address::Root::Derived(_)|address::Root::Resource{..}|address::Root::Dataset{..}|address::Root::External(address::Identity::Storage(_))))))}
    fn project(self: &Arc<Self>, root: usize, op: Op) -> Borrow {
        let node = &self.nodes[root];
        if matches!(op, Op::Resources | Op::Owners)
            && node.seeds.empty()
            && node.edges.len() == 1
            && node.edges[0].op == op
            && node.edges[0].when.boolean_constant() == Some(true)
        {
            return Borrow::source(Origin::Relation(Arc::clone(self), root));
        }

        let selected = Node {
            seeds: Borrow::default(),
            edges: vec![Edge { from: root, op, when: bounds::Condition::constant(true) }],
        };
        if let Some(id) = self.nodes.iter().position(|node| *node == selected) {
            return Borrow::source(Origin::Relation(Arc::clone(self), id));
        }

        let mut graph = self.as_ref().clone();
        let id = graph.nodes.len();
        graph.nodes.push(selected);
        Borrow::source(Origin::Relation(Arc::new(graph), id))
    }
    pub(in crate::check) fn resources(self: &Arc<Self>, root: usize) -> Borrow {
        self.project(root, Op::Resources)
    }
    pub(in crate::check) fn owners(self: &Arc<Self>, root: usize) -> Borrow {
        self.project(root, Op::Owners)
    }
    pub(in crate::check) fn select(self: &Arc<Self>, root: usize, indices: &[Option<bounds::Operand>]) -> Borrow {
        self.project(root, Op::Select(indices.to_vec()))
    }
    pub(in crate::check) fn seed_borrows(&self,root:usize)->Vec<&Borrow>{self.reachable(root).iter().map(|&id|&self.nodes[id].seeds).collect()}
    pub(in crate::check) fn public(&self)->Self{let mut v=self.clone();v.inputs.clear();for n in &mut v.nodes{for e in &mut n.edges{if let Op::Read{epoch,..}=&mut e.op{*epoch=Epoch::Input;}}}v}
    pub(in crate::check) fn at_call(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{let mut v=self.clone();for n in &mut v.nodes{n.seeds=n.seeds.at_call(values);for e in &mut n.edges{e.when=e.when.guard_at_call(values);match &mut e.op{Op::Position(p)=>*p=p.bound_at_call(values),Op::Element{index,..}=>*index=Origin::index_at_call(*index,values),_=>{}}}}for f in &mut v.frames{for w in &mut f.writes{w.when=w.when.at_call(values);}}v}
    pub(in crate::check) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{let mut v=self.clone();for n in &mut v.nodes{n.seeds=n.seeds.substitute_values(values);for e in &mut n.edges{e.when=e.when.substitute_values(values);match &mut e.op{Op::Position(p)=>*p=p.substitute_values(values),Op::Element{index,..}=>*index=Origin::index_substitute(*index,values),_=>{}}}}for f in &mut v.frames{for w in &mut f.writes{w.when=w.when.substitute_values(values);}}v}
    pub(in crate::check) fn output_bound(&self,values:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{let mut v=self.clone();for n in &mut v.nodes{n.seeds=n.seeds.output_bound(values);for e in &mut n.edges{e.when=bounds::Condition::guard(e.when.assume(&bounds::Clause::default(),true).output_bound(values));if let Op::Position(p)=&mut e.op{*p=p.bind_outputs(values);}}}for f in &mut v.frames{for w in &mut f.writes{w.when=w.when.output_bound(values);}}v}
    pub(in crate::check) fn normalize(&mut self,normal:&mut bounds::Normalizer){for n in &mut self.nodes{n.seeds.normalize_contract(normal);for e in &mut n.edges{e.when=normal.condition(&e.when);if let Op::Position(p)=&mut e.op{normal.origin(p);}}}for f in &mut self.frames{for w in &mut f.writes{w.when=normal.condition(&w.when);}}}
    pub(super) fn canonical(
        &self, function: &Function, exposed: &BTreeMap<Origin, Borrow>,
        parameters: &BTreeMap<ValueIdentity, bounds::Proof>,
    ) -> Self {
        let mut public = self.public();
        let input_parameters = residency::input_parameters(&function.parameters, function.input_width);
        for node in &mut public.nodes {
            node.seeds = canonical_relation(node.seeds.clone(), function, None, exposed, parameters, false);
            for edge in &mut node.edges {
                edge.when = edge.when.guard_at_call(&input_parameters);
                if let Op::Position(origin) = &mut edge.op {
                    let borrow = Borrow::source(Origin::Position(Arc::new(origin.clone())));
                    let value = canonical_relation(borrow, function, None, exposed, parameters, false);
                    if let Some(Origin::Position(position)) = value.sources.iter().next() {
                        *origin = position.as_ref().clone();
                    }
                }
            }
        }
        for frame in &mut public.frames {
            for write in &mut frame.writes {
                write.when = write.when.at_call(&input_parameters);
            }
        }
        public
    }
    fn extension(&self,other:&Graph)->bool{other.nodes.get(..self.nodes.len())==Some(self.nodes.as_slice())&&other.frames==self.frames&&other.variables==self.variables&&other.inputs==self.inputs}
    fn references(&self,source:&Origin)->bool{
        let borrow=|b:&Borrow|b.sources.iter().any(|s|self.references(s));
        match source{
            Origin::Relation(g,_)=>self.extension(g),
            Origin::Element{parent,..}|Origin::Projection{parent,..}=>borrow(parent),
            Origin::Position(p)|Origin::Contents{origin:p,..}=>match &p.root{address::Root::Derived(b)|address::Root::Resource{borrow:b,..}|address::Root::Dataset{borrow:b,..}|address::Root::External(address::Identity::Storage(b))=>borrow(b),_=>false},
            _=>false,
        }
    }
    fn private(&self,source:&Origin)->bool{!self.references(source)&&Borrow::source(source.clone()).has_local()}
    fn residual(&self,root:usize,query:&Query<'_,'_,'_>)->Self{
        let mut residual=self.clone();let mut normal=bounds::Normalizer::new();
        for &id in query.values.keys() {
            if let Cow::Owned(node) = query.local_definition(id) {
                residual.nodes[id] = node;
            }
        }
        for(id,node)in residual.nodes.iter_mut().enumerate(){
            node.seeds=node.seeds.map_sources(|s|if self.private(&s){Borrow::default()}else{Borrow::source(s)});
            if self.variables.contains(&id)||self.nodes[id].edges.iter().any(|e|matches!(e.op,Op::Read{..})) {
                if !query.variables.contains_key(&id)&&!query.values.contains_key(&id){continue;}
                for(source,when)in query.node(id).sources.entries(){
                    if !self.private(source)&&!self.references(source){node.seeds.sources.insert_normalized(source.clone(),when,&mut normal);}
                }
            }
        }
        let frames=query.values.keys().chain(query.variables.keys()).flat_map(|id|self.frames_at(*id)).collect::<BTreeSet<_>>();
        for id in frames{
            let frame=&mut residual.frames[id];
            frame.writes.retain(|write|query.node(write.target).sources.entries().any(|(source,when)|when.possible()&&!self.private(source)));
            for write in &frame.writes{for(_,id)in write.fields.iter().flatten(){for(source,when)in query.node(*id).sources.entries(){
                if self.private(source){residual.nodes[*id].seeds.sources.insert_normalized(source.clone(),when,&mut normal);}
            }}}
        }
        if let Some(observe) = query.evaluation.known.observe {
            observe(query.evaluation.state.name, crate::compiler::BorrowSolveStorage::Residual {
                root, tables: normal.storage(),
            });
        }
        residual
    }
    fn rebind<'r>(&self,value:&Borrow,residual:&impl Fn()->&'r Graph)->Borrow{
        value.map_sources(|source|match source{
            Origin::Relation(g,root)if self.extension(&g)=>{
                let residual = residual();
                let mut graph=(*g).clone();graph.nodes[..self.nodes.len()].clone_from_slice(&residual.nodes);graph.frames=residual.frames.clone();
                Borrow::source(Origin::Relation(Arc::new(graph),root))
            },
            Origin::Element{parent,fields,index}=>Borrow::element(self.rebind(&parent,residual),fields,index),
            Origin::Projection{parent,fields}=>Borrow::projection(self.rebind(&parent,residual),fields),
            Origin::Position(p)=>p.map_borrows(|b|self.rebind(b,residual)).normalize(),
            Origin::Contents{origin,word}=>Borrow::source(Origin::Contents{origin:Arc::new(origin.map_borrows(|b|self.rebind(b,residual))),word}),
            other=>Borrow::source(other),
        })
    }
    pub(super) fn solve(self: &Arc<Self>, root: usize, evaluation: &mut BorrowEvaluation<'_>) -> Borrow {
        let reachable = self.reachable(root);
        let (mut stable, feedback): (Vec<_>, Vec<_>) =
            self.order(&reachable).into_iter().partition(|&(_, changes)| !changes);
        let mut variables = self.variables.intersection(&reachable)
            .map(|&id| (id, Borrow::default())).collect::<BTreeMap<_, _>>();
        let mut values = BTreeMap::new();
        loop {
            let mut scope = BorrowEvaluation {
                function: evaluation.function,
                state: evaluation.state,
                known: evaluation.known,
                expand_loops: evaluation.expand_loops,
                deferred_reads: evaluation.deferred_reads,
                loop_bindings: evaluation.loop_bindings.clone(),
                values: BTreeMap::new(),
            };
            scope.loop_bindings.push(self.inputs.iter().filter_map(|(input, id)| {
                variables.get(id).map(|value| (*input, value.clone()))
            }).collect());

            let mut query = Query {
                graph: self,
                relation: Arc::clone(self),
                variables: &variables,
                values,
                evaluation: &mut scope,
            };
            for (id, _) in stable.drain(..).chain(feedback.iter().copied()) {
                let value = query.definition(id);
                query.values.insert(id, value);
            }
            let next = variables.keys().map(|&id| (id, query.definition(id)))
                .collect::<BTreeMap<_, _>>();
            if let Some(observe) = query.evaluation.known.observe {
                use crate::compiler::{AnalysisStorage, BorrowSolveStorage};
                observe(query.evaluation.state.name, BorrowSolveStorage::Relation {
                    root,
                    nodes: AnalysisStorage::vector(&self.nodes),
                    frames: AnalysisStorage::vector(&self.frames),
                    resolved: AnalysisStorage::entries::<usize, Borrow>(query.values.len(), None),
                    previous_inputs: AnalysisStorage::entries::<usize, Borrow>(variables.len(), None),
                    next_inputs: AnalysisStorage::entries::<usize, Borrow>(next.len(), None),
                });
            }
            if variables.iter().all(|(id, previous)| next[id].sources.same_sources(&previous.sources))
                && variables.iter().all(|(id, previous)| next[id].sources.equivalent_paths(&previous.sources))
            {
                let value = query.node(root);
                let residual = std::cell::OnceCell::new();
                return self.rebind(&value, &|| residual.get_or_init(|| self.residual(root, &query)));
            }
            values = query.values;
            for &(id, _) in &feedback {
                values.remove(&id);
            }
            variables = next;
        }
    }
}
struct Query<'g,'e,'f>{graph:&'g Graph,relation:Arc<Graph>,variables:&'g BTreeMap<usize,Borrow>,values:BTreeMap<usize,Borrow>,evaluation:&'e mut BorrowEvaluation<'f>}
impl<'g> Query<'g, '_, '_> {
    fn node(&self,id:usize)->Borrow{self.variables.get(&id).unwrap_or_else(||&self.values[&id]).clone()}
    fn definition(&mut self, id: usize) -> Borrow {
        let mut definition = self.local_definition(id);
        let mut sources = match &mut definition {
            Cow::Borrowed(node) => node.seeds.sources.clone(),
            Cow::Owned(node) => std::mem::take(&mut node.seeds.sources),
        };
        for edge in &definition.edges {
            let from = self.node(edge.from);
            let incoming = self.apply(id, edge.from, &edge.op, &from);
            let path = edge.when.assume(&bounds::Clause::default(), true);
            sources.union(&incoming.sources.restricted(&path));
        }
        Borrow { sources, ..Borrow::default() }
    }
    fn deferred(&self,id:usize)->Borrow{Borrow::source(Origin::Relation(self.relation.clone(),id))}
    fn apply(&mut self,id:usize,from_id:usize,op:&Op,from:&Borrow)->Borrow{
        if let Op::Read{word,epoch,frame}=op{return self.read(id,from_id,from,*word,epoch,*frame);}
        from.map_sources(|source|match op{Op::Identity=>Borrow::source(source),Op::Resources=>Borrow::source(source).resources(),Op::Owners=>Borrow::source(source).owners(),Op::Select(indices)=>Borrow::source(source).select_elements(indices),Op::Element{fields,index}=>Borrow::element(Borrow::source(source),fields.clone(),*index),Op::Projection(fields)=>Borrow::projection(Borrow::source(source),fields.clone()),Op::Position(p)=>p.map_borrows(|_|Borrow::source(source)).normalize(),Op::Read{..}|Op::Initial{..}=>Borrow::default()})
    }
    fn local_definition(&self, id: usize) -> Cow<'g, Node> {
        let node = &self.graph.nodes[id];
        if !node.edges.iter().any(|edge| matches!(edge.op, Op::Initial { .. })) {
            return Cow::Borrowed(node);
        }
        let mut result = Node { seeds: node.seeds.clone(), edges: Vec::with_capacity(node.edges.len()) };
        for edge in &node.edges {
            let Op::Initial{origin,frame}=&edge.op else{result.edges.push(edge.clone());continue;};
            let path = edge.when.assume(&bounds::Clause::default(), true);
            let position=content_position(origin,0);let mut earlier=bounds::Condition::constant(true);let mut at=*frame;
            while let Some(fi)=at{
                let f=&self.graph.frames[fi];at=f.parent;
                for write in f.writes.iter().rev() {
                    let targets = self.node(write.target);
                    let targets = targets.sources.entries().filter_map(|(target, selected)| {
                        let yes = write.when.assume(&bounds::Clause::both(&path, &selected), true);
                        yes.possible().then_some((target, yes))
                    });
                    let Some(fields) = &write.fields else {
                        // A possible external write contributes a source while retaining the earlier value.
                        for (target, yes) in targets {
                            let affected = match target {
                                Origin::Position(target) if target.symbolic_input() && !target.has_local() => false,
                                Origin::Position(target) => address::MemoryEffect::of(
                                    &Borrow::source(Origin::Position(target.clone())), super::super::Access::Write, None,
                                ).written_bytes(position.as_ref().unwrap(), Scalar::U64, None) != 0,
                                Origin::External => true,
                                _ => false,
                            };
                            if affected {
                                result.seeds.union(&Borrow::source(Origin::External).restricted(&earlier.assume(&yes, true)));
                            }
                        }
                        continue;
                    };

                    let mut hit = bounds::Clause::impossible();
                    let mut miss = bounds::Clause::impossible();
                    for (target, yes) in targets {
                        let mut complete = false;
                        if let Origin::Position(target) = target {
                            // A formal input cannot reach this invocation's private slots.
                            if target.symbolic_input() && !target.has_local() {
                                miss = bounds::Clause::either(&miss, &yes);
                                continue;
                            }
                            for (word, value) in fields {
                                let (affected, full) = content_write_overlap(
                                    Some((origin, 0)), position.as_ref(), target, *word, &yes,
                                );
                                if affected {
                                    result.edges.push(Edge {
                                        from: *value,
                                        op: Op::Identity,
                                        when: {
                                            let visible = earlier.clone()
                                                .logical(bounds::Condition::guard(yes.clone()), false);
                                            // The selected write already includes the read path.
                                            bounds::Condition::paths(
                                                visible.assume(&bounds::Clause::default(), true),
                                                bounds::Clause::either(
                                                    &visible.assume(&bounds::Clause::default(), false),
                                                    &edge.when.assume(&bounds::Clause::default(), false),
                                                ),
                                            )
                                        },
                                    });
                                    hit = bounds::Clause::either(&hit, &yes);
                                    complete |= full;
                                }
                            }
                        } else if matches!(target, Origin::External) {
                            result.seeds.union(&Borrow::source(Origin::External).restricted(&earlier.assume(&yes, true)));
                        }
                        if !complete {
                            miss = bounds::Clause::either(&miss, &yes);
                        }
                    }
                    let no = bounds::Clause::either(&write.when.assume(&path, false), &miss);
                    earlier = earlier.logical(bounds::Condition::paths(hit, no).negate(), false);
                }
            }
            result.edges.push(Edge{from:edge.from,op:Op::Identity,when:earlier.logical(edge.when.clone(),false)});
        }
        Cow::Owned(result)
    }
    fn binding(&self,mut id:usize)->usize{
        loop{let n=self.local_definition(id);if !self.graph.variables.contains(&id)&&n.seeds.empty()&&n.edges.len()==1&&n.edges[0].op==Op::Identity{id=n.edges[0].from;}else{return id;}}
    }
    fn location(&self,id:usize)->Option<(usize,address::Origin)>{let id=self.binding(id);let n=self.local_definition(id);if n.seeds.empty()&&n.edges.len()==1&&let Op::Position(origin)=&n.edges[0].op{Some((self.binding(n.edges[0].from),origin.clone()))}else{None}}
    fn same_word(&self,read:usize,word:u32,write:usize,field:u32)->bool{
        let(Some((a,x)),Some((b,y)))=(self.location(read),self.location(write))else{return false;};
        (a==b||(!self.graph.variables.contains(&a)&&!self.graph.variables.contains(&b)&&self.graph.nodes[a]==self.graph.nodes[b]))&&x.root==y.root&&x.base==y.base&&content_word(&x,word).zip(content_word(&y,field)).is_some_and(|((_,a),(_,b))|a==b)
    }
    fn read(&mut self,id:usize,from_id:usize,from:&Borrow,word:u32,epoch:&Epoch,frame:Option<usize>)->Borrow{
        let mut output=Borrow::default();
        for(source,path)in from.sources.entries(){
            let Origin::Position(origin)=source else{continue;};let position=content_position(origin,word);let mut content=ContentAtRead::default();let mut at=frame;let mut delayed=false;let mut ready=true;
            while let Some(fi)=at{
                let frame = &self.graph.frames[fi];
                at = frame.parent;
                for write in frame.writes.iter().rev() {
                    let yes=write.when.assume(&path,true);if !yes.possible(){continue;}
                    let fields=write.fields.iter().flatten().filter(|(field,_)|self.same_word(from_id,word,write.target,*field)).collect::<Vec<_>>();
                    if !fields.is_empty(){
                        let mut incoming=Borrow::default();for(_,value)in fields{let v=self.node(*value);if v.empty(){ready=false;break;}incoming.sources.union(&v.sources);}
                        content.write(&incoming,&write.when,true);continue;
                    }
                    let targets=self.node(write.target);if targets.empty(){ready=false;break;}
                    let mut hit=bounds::Clause::impossible();let mut miss=bounds::Clause::impossible();let mut incoming=Borrow::default();
                    for(target,selected)in targets.sources.entries(){
                        let selected=bounds::Clause::both(&yes,&selected);if !selected.possible(){continue;}
                        let mut complete=false;let mut possible=false;
                        if let Origin::Position(target)=target{
                            if let Some(fields)=&write.fields{
                                for (field,value)in fields{
                                    let(affected,full)=content_write_overlap(Some((origin,word)),position.as_ref(),target,*field,&selected);
                                    if affected{let v=self.node(*value);if v.empty(){ready=false;break;}incoming.sources.union(&v.sources.restricted(&selected));possible=true;complete|=full;}
                                    else if !full&&(origin.symbolic_input()||target.symbolic_input()||matches!(origin.root,address::Root::Parameter(_))||matches!(target.root,address::Root::Parameter(_))){delayed=true;}
                                }
                            }else if position.as_ref().is_some_and(|p|address::MemoryEffect::of(&Borrow::source(Origin::Position(target.clone())).restricted(&selected),super::super::Access::Write,None).written_bytes(p,Scalar::U64,None)!=0){possible=true;incoming.sources.union(&Borrow::source(Origin::External).sources.restricted(&selected));}
                        }else if matches!(target,Origin::External){possible=true;incoming.sources.union(&Borrow::source(Origin::External).sources.restricted(&selected));}
                        if possible{hit=bounds::Clause::either(&hit,&selected);}if !complete{miss=bounds::Clause::either(&miss,&selected);}
                    }
                    let no=bounds::Clause::either(&write.when.assume(&path,false),&miss);content.write(&incoming,&bounds::Condition::paths(hit,no),true);
                }
                if !ready{break;}
            }
            let value=if !ready{Borrow::default()}else if delayed{self.deferred(id)}else if content.earlier.as_ref().is_some_and(|p|!p.possible()){content.with_initial(Borrow::default())}else if origin.symbolic_input(){self.deferred(id)}else{
                let base=match epoch{Epoch::Input=>input_contents(origin,word),Epoch::Local{memory,snapshots}=>self.evaluation.content_before(origin,word,*memory,snapshots.as_deref())};
                if base.sources.iter().any(|s|matches!(s,Origin::Contents{..})){self.deferred(id)}else{content.with_initial(base)}
            };output.sources.union(&value.sources.restricted(&path));
        }output
    }
}
