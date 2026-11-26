module bootstrap.task_completion;

enum Phase:u32 { Begin=0; Position=1; AfterPosition=2; Owner=3; AfterOwner=4; Dataset=5; AfterDataset=6; CloseDataset=7; };
struct Cursor { Phase phase; usize index; usize owner; };
struct Completion { u32 result_owns; usize positions; usize datasets; };
struct Ready { u32 present; usize owners; };
struct Dataset { u32 writes; usize owners; };
enum Need:u32 { Result=0; Position=1; Owner=2; Dataset=3; Complete=4; };
struct Release { Cursor cursor; Need need; };
struct Host {
 cfn<void,address> begin_scope;
 cfn<void,address,u32> end_scope;
 cfn<Ready,address,usize> position;
 cfn<void,address,usize> begin_owner;
 cfn<void,address> end_owner;
 cfn<Dataset,address,usize> dataset;
 cfn<void,address,usize> descriptor;
 cfn<void,address,usize,usize> guard;
};
extern "C" Release next_release(Cursor state,Completion completion,address context,Host host) on cpu
 requires nonzero(host.begin_scope) requires nonzero(host.end_scope) requires nonzero(host.position)
 requires nonzero(host.begin_owner) requires nonzero(host.end_owner) requires nonzero(host.dataset)
 requires nonzero(host.descriptor) requires nonzero(host.guard);
