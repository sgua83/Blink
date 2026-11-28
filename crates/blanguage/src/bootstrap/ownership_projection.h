module bootstrap.ownership_projection;
import bootstrap.ast_storage;
struct Source {address nodes;address source;address stored;ExprRef expression;};
struct Bytes {address data;u64 count;};
enum PlaceRootKind:u32 { Other=0; Named=1; Address=2; Temporary=3; };
struct PlaceRoot { Bytes name; PlaceRootKind kind; };
struct TypeInfo {address array;u64 length;u64 fields;u64 components;};
struct Field {Bytes name;u32 offset;address ty;};
struct Part {u32 offset;address ty;};
struct KnownIndex {u32 present;u32 value;};
struct Projection {address ty;Text root;u32 offset;};
enum LoopPlace:u32 { Missing=0; Root=1; Child=2; };
struct LoopSelection { address ty; LoopPlace place; };
struct Host {
    cfn<address,address,Bytes> root;
    cfn<KnownIndex,address,Bytes> index;
    cfn<TypeInfo,address> ty;
    cfn<Field,address,u64> field;
    cfn<Part,address,u64> element;
    cfn<address,address,Bytes> selected_field;
    cfn<u32,address,u64> word;
    cfn<void,address,u64> push;
};
extern "C" Projection owned(Source source,address env,address path,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.index) requires nonzero(host.ty)
    requires nonzero(host.field) requires nonzero(host.element) requires nonzero(host.selected_field) requires nonzero(host.push);
extern "C" address owned_type(Source source,address env,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.index) requires nonzero(host.ty)
    requires nonzero(host.field) requires nonzero(host.element) requires nonzero(host.selected_field) requires nonzero(host.push);
extern "C" LoopSelection loop_projection(Source source,address context,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.index) requires nonzero(host.ty)
    requires nonzero(host.field) requires nonzero(host.element) requires nonzero(host.selected_field) requires nonzero(host.push);
extern "C" PlaceRoot place_root(Source source) on cpu;
extern "C" Part field_layout(address root,address fields,u64 count,Host host) on cpu
    requires nonzero(host.element);
extern "C" void components(address root,address selected,u32 offset,address output,Host host) on cpu
    requires nonzero(host.ty) requires nonzero(host.word) requires nonzero(host.push);
