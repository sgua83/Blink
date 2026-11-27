module bootstrap.capture_inputs;
import bootstrap.names;
import bootstrap.declarations;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize size) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,usize size) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";

const u32 CAPTURE_OUT_OF_MEMORY=8;

extern "C" void discard_capture_value(address value) on cpu {}

struct CaptureStep { CaptureImports imports; u32 status; };

CaptureStep open_capture(CaptureHost host, address visited, NameBytes name) on cpu
    requires nonzero(host.open) requires nonzero(host.bind) {
    CaptureOpened opened = host.open(host.context, name);
    CaptureImports empty=CaptureImports(address(),0u64);
    if (opened.status == CaptureStatus::Failed) { return CaptureStep(empty,0); }
    if (opened.status == CaptureStatus::Ignored) { return CaptureStep(empty,1); }
    // The catalog owner is a nonzero marker for every present module name.
    Insertion added=insert(visited,opened.name,visited);
    if (added.success==0) { return CaptureStep(empty,CAPTURE_OUT_OF_MEMORY); }
    return CaptureStep(host.bind(host.context,name,u32(added.replaced==address())),1);
}

struct CaptureFrame { CaptureImports imports; usize next; };
struct CaptureStack { address rows; usize count; usize capacity; };

address capture_frame(address rows,usize index) on cpu {
    return unsafe_address(address_bits(rows)+index*size_of<CaptureFrame>());
}

bool push_capture(inout CaptureStack stack,handle heap,CaptureImports imports) on cpu {
    if (stack.count==stack.capacity) {
        usize limit=18446744073709551615u64/size_of<CaptureFrame>();
        if (stack.capacity==limit) { return false; }
        usize capacity=stack.capacity==0u64 ? 4u64 : stack.capacity+min(stack.capacity,limit-stack.capacity);
        usize size=capacity*size_of<CaptureFrame>();
        address rows=stack.capacity==0u64 ? HeapAlloc(heap,0,size) : HeapReAlloc(heap,0,stack.rows,size);
        if (rows==address()) { return false; }
        stack.rows=rows;
        stack.capacity=capacity;
    }
    unsafe_position<CaptureFrame>(capture_frame(stack.rows,stack.count))=CaptureFrame(imports,0u64);
    stack.count+=1u64;
    return true;
}

void release_capture(inout CaptureStack stack,handle heap,cfn<void,CaptureImports> close) on cpu
    requires nonzero(close) {
    while(stack.count!=0u64) {
        stack.count-=1u64;
        CaptureFrame frame=unsafe_position<CaptureFrame>(capture_frame(stack.rows,stack.count));
        close(frame.imports);
    }
    if(stack.rows!=address()){HeapFree(heap,0,stack.rows);}
}

u32 capture_branch(CaptureHost host, address visited, NameBytes name) on cpu
    requires nonzero(host.open) requires nonzero(host.bind)
    requires nonzero(host.name) requires nonzero(host.close) {
    CaptureStep step=open_capture(host,visited,name);
    CaptureImports imports=step.imports;
    if (imports.count == 0u64) { return step.status; }
    handle heap=GetProcessHeap();
    CaptureStack active=CaptureStack(address(),0u64,0u64);
    if (!push_capture(active,heap,imports)) {
        host.close(imports);
        return CAPTURE_OUT_OF_MEMORY;
    }
    u32 status=1;
    while (active.count!=0u64) {
        address slot=capture_frame(active.rows,active.count-1u64);
        CaptureFrame frame=unsafe_position<CaptureFrame>(slot);
        if (frame.next==frame.imports.count) {
            active.count-=1u64;
            host.close(frame.imports);
            continue;
        }
        NameBytes next=host.name(frame.imports,frame.next);
        unsafe_position<CaptureFrame>(slot).next+=1u64;
        if (find(visited,next)!=address()) { continue; }
        CaptureStep child=open_capture(host,visited,next);
        status=child.status;
        if (status!=1) { break; }
        if (child.imports.count!=0u64 && !push_capture(active,heap,child.imports)) {
            host.close(child.imports);
            status=CAPTURE_OUT_OF_MEMORY;
            break;
        }
    }
    release_capture(active,heap,host.close);
    return status;
}

extern "C" i32 capture_build_inputs(CaptureHost host, usize requests) on cpu
    requires nonzero(host.open) requires nonzero(host.bind)
    requires nonzero(host.name) requires nonzero(host.close) requires nonzero(host.export) {
    Catalog visited=Catalog(address(),0u64,0u64,address());
    // The export ABI borrows the catalog only for synchronous re-entry.
    address names=unsafe_address(address_bits(address_of(visited)));
    u32 complete=capture_branch(host,names,NameBytes(address(),0u64));
    usize index = 0u64;
    while (index<requests && complete==1) {
        complete=host.export(host.context,names,index);
        index += 1u64;
    }
    release(names,c_function(discard_capture_value));
    // Source failures are held in the original snapshot for ordered replay.
    return complete==CAPTURE_OUT_OF_MEMORY ? i32(CAPTURE_OUT_OF_MEMORY) : i32(0);
}

extern "C" u32 capture_export_owner(CaptureHost host, address visited, NameBytes name) on cpu
    requires nonzero(host.open) requires nonzero(host.bind)
    requires nonzero(host.name) requires nonzero(host.close) requires nonzero(host.provider) {
    if (host.provider(host.context,name)==0 || find(visited,name)!=address()) { return 1; }
    u32 result=capture_branch(host,visited,name);
    return result==1 ? 2 : result;
}
