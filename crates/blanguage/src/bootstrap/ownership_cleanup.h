module bootstrap.ownership_cleanup;
enum Kind:u32 {Unknown=0;Unit=1;Value=2;Owned=3;Data=4;};
struct Info {u32 flags;u64 count;};
struct Field {u32 word;address ty;};
struct Host {
 cfn<Info,address> inspect;
 cfn<void,address,u32,address,address,Info> selected;
 cfn<void,address,u32,address,address> fields;
 cfn<address,address,u64> child;
 cfn<u32,address,u64,u32> offset;
 cfn<u32,address,u64> word;
 cfn<Field,address,u64> field;
 cfn<void,address,u64,address,address> emit;
};
struct Cursor {u32 phase;u64 index;};
struct Step<T> {Cursor cursor;T need;};
enum OwnerNeed:u32 {Views=0;Children=1;Restrict=2;Union=3;Complete=4;};
enum ChildNeed:u32 {Inspect=0;Data=1;Owned=2;Complete=3;Next=4;};
struct ChildFacts {u64 count;Kind kind;u32 active;};
enum DataNeed:u32 {Inspect=0;Owned=1;Data=2;Stored=3;Unresolved=4;Restrict=5;Union=6;Complete=7;Next=8;};
struct DataFacts {u32 active;u32 nonempty;Kind kind;u32 conditional;u32 stored;};
extern "C" Cursor start() on cpu;
extern "C" Kind binding(Kind kind) on cpu;
extern "C" Step<OwnerNeed> owner_step(Cursor state,Info info) on cpu;
extern "C" Step<ChildNeed> child_step(Cursor state,ChildFacts facts) on cpu;
extern "C" Step<DataNeed> data_step(Cursor state,DataFacts facts) on cpu;
extern "C" void views(address node,u32 offset,address path,address context,Info info,Host host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.selected) requires nonzero(host.fields)
 requires nonzero(host.child) requires nonzero(host.offset);
extern "C" void views_active(address node,u32 offset,address path,address context,Info info,Host host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.selected) requires nonzero(host.fields)
 requires nonzero(host.child) requires nonzero(host.offset);
extern "C" void views_selected(address node,u32 offset,address path,address context,Info info,Host host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.selected) requires nonzero(host.fields)
 requires nonzero(host.child) requires nonzero(host.offset);
extern "C" void fields(address list,u64 count,u32 offset,address path,address context,Host host) on cpu
 requires nonzero(host.word) requires nonzero(host.field) requires nonzero(host.emit);
