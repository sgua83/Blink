module applications.system_info;
import blink.windows.system;

u32 main() on cpu {
    SystemInfo info;
    GetNativeSystemInfo(info);
    if (!println(info.processor_architecture)
        || !println(info.page_size)
        || !println(address_bits(info.minimum_application_address))
        || !println(address_bits(info.maximum_application_address))
        || !println(info.active_processor_mask)
        || !println(info.number_of_processors)
        || !println(info.processor_type)
        || !println(info.allocation_granularity)
        || !println(info.processor_level)
        || !println(info.processor_revision)) {
        return 1;
    }
    return 0;
}
