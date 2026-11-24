module bootstrap.function_inputs;
import bootstrap.call_execution;
enum Resource:u32 {Done=0;Buffer=1;Acceleration=2;Texture=3;Keep=4;};
enum Parameter:u32 {Acceleration=0;Texture=1;Dataset=2;Tile=3;Ordinary=4;};
enum DomainParameter:u32 {Accept=0;Reject=1;PlainLayout=2;};
enum DomainAccessError:u32 {None=0;Texture=1;Acceleration=2;Dataset=3;Uniform=4;};
struct DomainBinding { Parameter form; DomainAccessError access_error; u32 view_check; };
enum Dataset:u32 {Device=0;Owned=1;Borrowed=2;};
enum ValueForm:u32 {OwnedRead=0;AtomicParameter=1;Reference=2;Value=3;};
enum PlaceForm:u32 {CAbi=0;Native=1;LocalRead=2;LocalWrite=3;};
enum RefBody:u32 {Atomic=0;OwnedInOut=1;OwnedOutput=2;ReadContent=3;Empty=4;};
enum Prepare:u32 {Elements=0;BindElements=1;Resources=2;Resolve=3;Assume=4;Arrays=5;Outputs=6;Completions=7;Done=8;};
struct Facts {u32 dataset;u32 acceleration;u32 texture;u32 tile;};
struct Mode {u32 application;u32 c_abi;u32 cooperative;};
struct Reference {u32 initialize;RefBody body;};
struct ResourceStep {Resource need;u32 next;};
extern "C" ResourceStep resource_step(u32 stage,Facts facts) on cpu;
extern "C" u32 uniform_bytes(u32 buffers) on cpu;

struct UniformField { u32 size; u32 alignment; };
extern "C" u32 layout_uniforms(u32 buffers, usize count, address context,
    cfn<UniformField,address,usize> ensures memory(read) field,
    cfn<void,address,usize,u32> place) on cpu
    requires nonzero(field) requires nonzero(place);

extern "C" Parameter parameter_form(Facts facts) on cpu;
extern "C" bool domain_result(u32 unit, usize arguments, u32 dataset, u32 tile, u32 view) on cpu;
extern "C" DomainParameter domain_parameter(Facts facts, CallAccess access) on cpu;
extern "C" DomainBinding domain_binding(Facts facts, CallAccess access, u32 gpu, u32 qualified) on cpu;
extern "C" Dataset dataset_form(u32 ordinary_cpu,u32 by_value) on cpu;
extern "C" ValueForm value_form(Mode mode,u32 read,u32 atomic,u32 by_value,u32 owned) on cpu;
extern "C" PlaceForm place_form(Mode mode,u32 read) on cpu;
extern "C" Reference reference_body(Mode mode,u32 reads,u32 atomic,u32 inout,u32 owns) on cpu;
extern "C" u32 start_actions(Mode mode) on cpu;
extern "C" Prepare prepare(u32 stage,Mode mode) on cpu;
extern "C" u32 return_task_layout(u32 application_entry,u32 c_abi,u32 has_result) on cpu;

enum ReferenceForm:u32 { Empty=0; Whole=1; Split=2; };
struct ReferenceLayout { ReferenceForm form; usize count; };
struct ReferenceFacts { u32 array; u32 record; u32 view; u32 dataset; u32 variant; u32 enumeration; };
extern "C" ReferenceLayout reference_layout(ReferenceFacts facts,usize components,u32 c_abi) on cpu;

struct ProgramCursor {u32 phase;u32 resource;};
struct ProgramFacts {u32 present;Facts shape;u32 buffers;};
enum ProgramNeed:u32 {Next=0;Buffer=1;Acceleration=2;Texture=3;Keep=4;Finish=5;};
struct ProgramStep {ProgramCursor cursor;ProgramNeed need;u32 uniform_bytes;};
extern "C" ProgramStep program_step(ProgramCursor cursor,ProgramFacts facts) on cpu;

struct BindingCursor {u64 index;u64 data;u64 texture;u64 acceleration;u32 phase;u32 prepare;};
struct BindingFacts {u32 present;Facts shape;};
enum BindingNeed:u32 {Frame=0;Calls=1;Cooperative=2;Next=3;Acceleration=4;Texture=5;Dataset=6;Type=7;Tile=8;Ordinary=9;Bind=10;Elements=11;BindElements=12;Resources=13;Resolve=14;Assume=15;Arrays=16;Outputs=17;Completions=18;Finish=19;};
struct BindingStep {BindingCursor cursor;BindingNeed need;};
extern "C" BindingStep binding_step(BindingCursor cursor,Mode mode,BindingFacts facts) on cpu;
extern "C" void resource_parameters(u64 count,address context,cfn<u32,address,u64> inspect,cfn<void,address,u64> publish) on cpu requires nonzero(inspect) requires nonzero(publish);

struct InputCursor {u32 phase;u64 index;};
struct DatasetFacts {u32 ordinary_cpu;u32 by_value;u32 inout;u64 components;};
enum DatasetNeed:u32 {Access=0;Components=1;Initialize=2;Device=3;Application=4;OwnedAbi=5;Rebound=6;Owned=7;Layout=8;Registers=9;BorrowedAbi=10;Borrowed=11;};
struct DatasetStep {InputCursor cursor;DatasetNeed need;};
extern "C" DatasetStep dataset_step(InputCursor cursor,DatasetFacts facts) on cpu;

struct OrdinaryFacts {u32 owned;u64 components;ReferenceFacts reference;};
enum OrdinaryNeed:u32 {Components=0;Register=1;ValueAbi=2;Value=3;Incoming=4;Borrow=5;Residency=6;OwnedValue=7;PlainValue=8;AtomicParameter=9;Origin=10;LocalRead=11;LocalWrite=12;ReferenceFacts=13;ReferenceWhole=14;ReferenceSplit=15;ReferenceWord=16;ReferencePlace=17;Initialize=18;AtomicReference=19;ReferenceType=20;ContentProof=21;EmptyProof=22;TemplateValue=23;TemplateReturned=24;OwnedReference=25;Reference=26;};
struct OrdinaryStep {InputCursor cursor;OrdinaryNeed need;ReferenceForm form;u32 inout;};
extern "C" OrdinaryStep ordinary_step(InputCursor cursor,Mode mode,CallAccess access,OrdinaryFacts facts) on cpu;

struct CompletionFacts {u32 present;u32 nested;u64 parent;u64 component;u64 input;};
enum CompletionNeed:u32 {Next=0;Publish=1;Finish=2;};
enum CompletionSource:u32 {Input=0;Returned=1;};
struct CompletionStep {InputCursor cursor;CompletionNeed need;CompletionSource source;u64 input;u64 component;};
extern "C" CompletionStep completion_step(InputCursor cursor,u64 first,CompletionFacts facts) on cpu;
struct ReturnFacts {u32 application;u32 c_abi;u32 result;u32 nonempty;};
enum ReturnNeed:u32 {Inspect=0;Publish=1;Finish=2;};
struct ReturnStep {InputCursor cursor;ReturnNeed need;};
extern "C" ReturnStep return_step(InputCursor cursor,ReturnFacts facts) on cpu;
