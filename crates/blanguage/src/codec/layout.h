module codec.layout;

const u32 LAYOUT_DATASET=1;
const u32 LAYOUT_VIEW=2;
const u32 LAYOUT_ARRAY=4;
const u32 LAYOUT_RESULT=8;
const u32 LAYOUT_ENUMERATION=16;
const u32 LAYOUT_CLEANUP=32;
const u32 LAYOUT_FUNCTION=64;
const u32 LAYOUT_VOID=128;
const u32 LAYOUT_VARIANT=256;
const u32 LAYOUT_RECORD=512;

struct LayoutText { address bytes; u64 length; };

struct LayoutNode {
    LayoutText name;
    u32 flags;
    u32 size;
    u32 align;
    u64 arguments;
    u64 values;
    u64 components;
};

struct LayoutComponent { LayoutText scalar; u32 offset; u32 size; u32 bit_width; };
struct LayoutField { LayoutText name; u32 offset; address value; };
struct LayoutVariant { LayoutText name; u32 discriminant; u32 offset; address value; };

enum LayoutPartKind : u32 {
    Done=0; Value=1; Fields=2; Variants=3; Array=4; View=5; Dataset=6; End=7; Close=8; Release=9;
};
struct LayoutPart { LayoutPartKind kind; address value; usize index; };

enum LayoutNeed : u32 { Done=0; Resolve=1; Release=2; };
struct LayoutRequest { LayoutNeed need; address value; };

struct LayoutSink {
    cfn<void,address,LayoutText> text;
    cfn<void,address,LayoutText> string;
    cfn<void,address,u64> number;
    cfn<void,address,LayoutPart> push;
    cfn<LayoutPart,address> pop;
};

struct LayoutSource {
    cfn<LayoutNode,address> node;
    cfn<LayoutText,address,u64> argument;
    cfn<u32,address,u64> value;
    cfn<LayoutComponent,address,u64> component;
    cfn<LayoutField,address,u64> field;
    cfn<LayoutVariant,address,u64> variant;
    cfn<address,address> array;
    cfn<address,address> view;
    cfn<void,address,address> dataset_name;
    cfn<LayoutText,address> cleanup;
};

struct LayoutProgram { LayoutText name; u32 target; u32 uniform_bytes; u64 buffers; u64 uniforms; };
struct LayoutBuffer { LayoutText name; u32 access; u32 stride; u32 count_present; u32 count; address element; };
struct LayoutUniform { LayoutText name; u32 offset; address value; };
struct LayoutInterface {
    cfn<LayoutProgram,address> owner;
    cfn<LayoutBuffer,address,u64> buffer;
    cfn<LayoutUniform,address,u64> uniform;
};

extern "C" void write_value(address out, address value, LayoutSink sink, LayoutSource source) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(sink.push) requires nonzero(sink.pop)
    requires nonzero(source.node) requires nonzero(source.argument) requires nonzero(source.value)
    requires nonzero(source.component) requires nonzero(source.field) requires nonzero(source.variant)
    requires nonzero(source.array) requires nonzero(source.view) requires nonzero(source.dataset_name)
    requires nonzero(source.cleanup);

extern "C" LayoutRequest next_reference(address out, LayoutSink sink, LayoutSource source) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(sink.push) requires nonzero(sink.pop)
    requires nonzero(source.node) requires nonzero(source.argument) requires nonzero(source.value)
    requires nonzero(source.component) requires nonzero(source.field) requires nonzero(source.variant)
    requires nonzero(source.array) requires nonzero(source.view) requires nonzero(source.dataset_name)
    requires nonzero(source.cleanup);

extern "C" void resolved_element(address out, address value, u32 repeated, LayoutSink sink, LayoutSource source) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(sink.push) requires nonzero(source.node);

extern "C" void write_interface(address out, address owner, LayoutSink sink, LayoutSource source,
    LayoutInterface declaration) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(sink.push) requires nonzero(sink.pop)
    requires nonzero(source.node) requires nonzero(source.argument) requires nonzero(source.value)
    requires nonzero(source.component) requires nonzero(source.field) requires nonzero(source.variant)
    requires nonzero(source.array) requires nonzero(source.view) requires nonzero(source.dataset_name)
    requires nonzero(source.cleanup) requires nonzero(declaration.owner)
    requires nonzero(declaration.buffer) requires nonzero(declaration.uniform);

enum SlotChildKind : u32 { Argument=0; Field=1; Variant=2; Element=3; };
struct SlotChild { address value; u32 offset; };
struct SlotType {
    usize arguments; usize fields; usize variants; usize components;
    u32 array; u32 count; u32 size; u32 alignment;
};
struct SlotHost {
    cfn<SlotType,address> shape;
    cfn<SlotChild,address,SlotChildKind,usize> child;
    cfn<void,address,usize,usize> prepare;
    cfn<void,address,address,u32> copy;
    cfn<void,address,u32> payload_word;
};

extern "C" void fill_value_slots(address value, u32 complete, SlotHost host) on cpu
    requires nonzero(host.shape) requires nonzero(host.child) requires nonzero(host.prepare)
    requires nonzero(host.copy) requires nonzero(host.payload_word);
