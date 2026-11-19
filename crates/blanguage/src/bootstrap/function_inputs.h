module bootstrap.function_inputs;
enum Resource:u32 {Done=0;Buffer=1;Acceleration=2;Texture=3;Keep=4;};
enum Parameter:u32 {Acceleration=0;Texture=1;Dataset=2;Tile=3;Ordinary=4;};
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
