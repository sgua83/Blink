//! Shared ABI for the B dataset release and native cleanup producers.
#[derive(Clone,Copy)]
#[repr(u32)] pub(super) enum Kind{Unit=0,Value=1,Owned=2,Data=3}
#[derive(Clone,Copy)]
#[repr(u32)] pub(super) enum Model{Known=0,Common=1,Selected=2,Unknown=3,Input=4,Cleanup=5}
#[derive(Clone,Copy)]
#[repr(u32)] pub(super) enum ParentNeed{Borrow=0,Position=1,Borrowed=2,Inspect=3,Refresh=4,Reference=5,Elements=6,ReferenceHandle=7,StoredHandle=8,Handle=9,Release=10,Complete=11}
#[derive(Clone,Copy)]
#[repr(u32)] pub(super) enum ElementNeed{Read=0,NativeModel=1,NativeDrop=2,KnownModel=3,Storage=4,KnownDrop=5}
#[derive(Clone,Copy)]
#[repr(u32)] pub(super) enum LoopNeed{Cpu=0,Initialize=1,Outer=2,Load=3,Zero=4,Positive=5,Condition=6,One=7,Previous=8,Place=9,Drop=10,Advance=11,Loop=12,Complete=13}
#[derive(Clone,Copy)]
#[repr(u32)] pub(super) enum NativeNeed{UnitType=0,ValueType=1,OwnedType=2,DataType=3,MovableData=4,MovableTree=5,Declare=6,Invoke=7,Next=8}
#[derive(Clone,Copy)]
#[repr(u32)] pub(super) enum DeclNeed{Name=0,Header=1,Declare=2,Body=3,Complete=4,Next=5}
#[derive(Clone,Copy)]
#[repr(u32)] pub(super) enum BodyNeed{Builder=1,Frame=2,Calls=3,Origin=4,DataParameter=5,DataAbi=6,DataBind=7,Registers=8,ValueAbi=9,Value=10,Inspect=11,Incoming=12,Residency=13,OwnedBind=14,ValueBind=15,TakeElementsEnv=16,ElementPaths=17,PutElementsEnv=18,BindElements=19,ResourceFacts=20,TakeContractEnv=21,Resolve=22,Assume=23,PutContractEnv=24,Arrays=25,Release=26,Initialization=27,Returns=28,Finish=29,Publish=30,Complete=31,Next=32}
#[derive(Clone,Copy)]
#[repr(C)] pub(super) struct Cursor{pub(super) phase:u32,pub(super) index:u64}
#[repr(C)] pub(super) struct Step<T>{pub(super) cursor:Cursor,pub(super) need:T}
#[derive(Clone,Copy,Default)]
#[repr(C)] pub(super) struct ParentFacts{pub(super) borrowed:u32,pub(super) children:u32,pub(super) nonempty:u32,pub(super) placed:u32}
#[derive(Clone,Copy)]
#[repr(C)] pub(super) struct DeclFacts{pub(super) made:u32}
#[derive(Clone,Copy,Default)]
#[repr(C)] pub(super) struct BodyFacts{pub(super) dataset:u32,pub(super) owned:u32}
unsafe extern "C"{
 #[link_name="bootstrap.dataset_drop::start"]pub(super) fn start()->Cursor;
 #[link_name="bootstrap.dataset_drop::index_maximum"]pub(super) fn index_maximum(maximum:u32)->u32;
 #[link_name="bootstrap.dataset_drop::parent_step"]pub(super) fn parent_step(state:Cursor,facts:ParentFacts)->Step<ParentNeed>;
 #[link_name="bootstrap.dataset_drop::element_step"]pub(super) fn element_step(state:Cursor,model:Model)->Step<ElementNeed>;
 #[link_name="bootstrap.dataset_drop::loop_step"]pub(super) fn loop_step(state:Cursor)->Step<LoopNeed>;
 #[link_name="bootstrap.dataset_drop::native_step"]pub(super) fn native_step(state:Cursor,kind:Kind)->Step<NativeNeed>;
 #[link_name="bootstrap.dataset_drop::resource_step"]pub(super) fn resource_step(state:Cursor,count:u64)->Step<u32>;
 #[link_name="bootstrap.dataset_drop::declaration_step"]pub(super) fn declaration_step(state:Cursor,facts:DeclFacts)->Step<DeclNeed>;
 #[link_name="bootstrap.dataset_drop::body_step"]pub(super) fn body_step(state:Cursor,facts:BodyFacts)->Step<BodyNeed>;
}
