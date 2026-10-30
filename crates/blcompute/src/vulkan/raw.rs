//! BLCompute 使用的 Vulkan 标准 ABI 子集。字段依据 Khronos vk.xml；不包含设备实现。
#![allow(non_snake_case, non_camel_case_types, dead_code)]
use std::ffi::{c_char, c_void};
pub type VkInstance = *mut c_void;
pub type VkPhysicalDevice = *mut c_void;
pub type VkDevice = *mut c_void;
pub type VkQueue = *mut c_void;
pub type VkCommandBuffer = *mut c_void;
pub type VkBuffer = u64;
pub type VkDeviceMemory = u64;
pub type VkSampler = u64;
pub type VkDescriptorSetLayout = u64;
pub type VkPipelineLayout = u64;
pub type VkShaderModule = u64;
pub type VkPipeline = u64;
pub type VkDescriptorPool = u64;
pub type VkDescriptorSet = u64;
pub type VkBufferView = u64;
pub type VkCommandPool = u64;
pub type VkFence = u64;
pub type VkSemaphore = u64;
pub type VkPipelineCache = u64;
#[repr(C)]
pub struct VkPhysicalDeviceFeatures2 {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub features: [u32; 55],
}
#[repr(C)]
pub struct VkPhysicalDevice8BitStorageFeatures {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub storageBuffer8BitAccess: u32,
    pub uniformAndStorageBuffer8BitAccess: u32,
    pub storagePushConstant8: u32,
}
#[repr(C)]
pub struct VkPhysicalDevice16BitStorageFeatures {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub storageBuffer16BitAccess: u32,
    pub uniformAndStorageBuffer16BitAccess: u32,
    pub storagePushConstant16: u32,
    pub storageInputOutput16: u32,
}
#[repr(C)]
pub struct VkPhysicalDeviceShaderAtomicInt64Features {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub shaderBufferInt64Atomics: u32,
    pub shaderSharedInt64Atomics: u32,
}
#[repr(C)]
pub struct VkPhysicalDeviceShaderFloat16Int8Features {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub shaderFloat16: u32,
    pub shaderInt8: u32,
}
#[repr(C)]
pub struct VkPhysicalDeviceBufferDeviceAddressFeatures {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub bufferDeviceAddress: u32,
    pub bufferDeviceAddressCaptureReplay: u32,
    pub bufferDeviceAddressMultiDevice: u32,
}
#[repr(C)]
pub struct VkPhysicalDeviceShaderSubgroupExtendedTypesFeatures {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub shaderSubgroupExtendedTypes: u32,
}
#[repr(C)]
pub struct VkPhysicalDeviceShaderMaximalReconvergenceFeaturesKHR {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub shaderMaximalReconvergence: u32,
}
#[repr(C)]
pub struct VkMemoryAllocateFlagsInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub deviceMask: u32,
}
#[repr(C)]
pub struct VkBufferDeviceAddressInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub buffer: VkBuffer,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkApplicationInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub pApplicationName: *const c_char,
    pub applicationVersion: u32,
    pub pEngineName: *const c_char,
    pub engineVersion: u32,
    pub apiVersion: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkInstanceCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub pApplicationInfo: *const VkApplicationInfo,
    pub enabledLayerCount: u32,
    pub ppEnabledLayerNames: *const *const c_char,
    pub enabledExtensionCount: u32,
    pub ppEnabledExtensionNames: *const *const c_char,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkDeviceQueueCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub queueFamilyIndex: u32,
    pub queueCount: u32,
    pub pQueuePriorities: *const f32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkDeviceCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub queueCreateInfoCount: u32,
    pub pQueueCreateInfos: *const VkDeviceQueueCreateInfo,
    pub enabledLayerCount: u32,
    pub ppEnabledLayerNames: *const *const c_char,
    pub enabledExtensionCount: u32,
    pub ppEnabledExtensionNames: *const *const c_char,
    pub pEnabledFeatures: *const c_void,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkQueueFamilyProperties {
    pub queueFlags: u32,
    pub queueCount: u32,
    pub timestampValidBits: u32,
    pub minImageTransferGranularity: VkExtent3D,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkExtent3D {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPhysicalDeviceLimits {
    pub maxImageDimension1D: u32,
    pub maxImageDimension2D: u32,
    pub maxImageDimension3D: u32,
    pub maxImageDimensionCube: u32,
    pub maxImageArrayLayers: u32,
    pub maxTexelBufferElements: u32,
    pub maxUniformBufferRange: u32,
    pub maxStorageBufferRange: u32,
    pub maxPushConstantsSize: u32,
    pub maxMemoryAllocationCount: u32,
    pub maxSamplerAllocationCount: u32,
    pub bufferImageGranularity: u64,
    pub sparseAddressSpaceSize: u64,
    pub maxBoundDescriptorSets: u32,
    pub maxPerStageDescriptorSamplers: u32,
    pub maxPerStageDescriptorUniformBuffers: u32,
    pub maxPerStageDescriptorStorageBuffers: u32,
    pub maxPerStageDescriptorSampledImages: u32,
    pub maxPerStageDescriptorStorageImages: u32,
    pub maxPerStageDescriptorInputAttachments: u32,
    pub maxPerStageResources: u32,
    pub maxDescriptorSetSamplers: u32,
    pub maxDescriptorSetUniformBuffers: u32,
    pub maxDescriptorSetUniformBuffersDynamic: u32,
    pub maxDescriptorSetStorageBuffers: u32,
    pub maxDescriptorSetStorageBuffersDynamic: u32,
    pub maxDescriptorSetSampledImages: u32,
    pub maxDescriptorSetStorageImages: u32,
    pub maxDescriptorSetInputAttachments: u32,
    pub maxVertexInputAttributes: u32,
    pub maxVertexInputBindings: u32,
    pub maxVertexInputAttributeOffset: u32,
    pub maxVertexInputBindingStride: u32,
    pub maxVertexOutputComponents: u32,
    pub maxTessellationGenerationLevel: u32,
    pub maxTessellationPatchSize: u32,
    pub maxTessellationControlPerVertexInputComponents: u32,
    pub maxTessellationControlPerVertexOutputComponents: u32,
    pub maxTessellationControlPerPatchOutputComponents: u32,
    pub maxTessellationControlTotalOutputComponents: u32,
    pub maxTessellationEvaluationInputComponents: u32,
    pub maxTessellationEvaluationOutputComponents: u32,
    pub maxGeometryShaderInvocations: u32,
    pub maxGeometryInputComponents: u32,
    pub maxGeometryOutputComponents: u32,
    pub maxGeometryOutputVertices: u32,
    pub maxGeometryTotalOutputComponents: u32,
    pub maxFragmentInputComponents: u32,
    pub maxFragmentOutputAttachments: u32,
    pub maxFragmentDualSrcAttachments: u32,
    pub maxFragmentCombinedOutputResources: u32,
    pub maxComputeSharedMemorySize: u32,
    pub maxComputeWorkGroupCount: [u32; 3],
    pub maxComputeWorkGroupInvocations: u32,
    pub maxComputeWorkGroupSize: [u32; 3],
    pub subPixelPrecisionBits: u32,
    pub subTexelPrecisionBits: u32,
    pub mipmapPrecisionBits: u32,
    pub maxDrawIndexedIndexValue: u32,
    pub maxDrawIndirectCount: u32,
    pub maxSamplerLodBias: f32,
    pub maxSamplerAnisotropy: f32,
    pub maxViewports: u32,
    pub maxViewportDimensions: [u32; 2],
    pub viewportBoundsRange: [f32; 2],
    pub viewportSubPixelBits: u32,
    pub minMemoryMapAlignment: usize,
    pub minTexelBufferOffsetAlignment: u64,
    pub minUniformBufferOffsetAlignment: u64,
    pub minStorageBufferOffsetAlignment: u64,
    pub minTexelOffset: i32,
    pub maxTexelOffset: u32,
    pub minTexelGatherOffset: i32,
    pub maxTexelGatherOffset: u32,
    pub minInterpolationOffset: f32,
    pub maxInterpolationOffset: f32,
    pub subPixelInterpolationOffsetBits: u32,
    pub maxFramebufferWidth: u32,
    pub maxFramebufferHeight: u32,
    pub maxFramebufferLayers: u32,
    pub framebufferColorSampleCounts: u32,
    pub framebufferDepthSampleCounts: u32,
    pub framebufferStencilSampleCounts: u32,
    pub framebufferNoAttachmentsSampleCounts: u32,
    pub maxColorAttachments: u32,
    pub sampledImageColorSampleCounts: u32,
    pub sampledImageIntegerSampleCounts: u32,
    pub sampledImageDepthSampleCounts: u32,
    pub sampledImageStencilSampleCounts: u32,
    pub storageImageSampleCounts: u32,
    pub maxSampleMaskWords: u32,
    pub timestampComputeAndGraphics: u32,
    pub timestampPeriod: f32,
    pub maxClipDistances: u32,
    pub maxCullDistances: u32,
    pub maxCombinedClipAndCullDistances: u32,
    pub discreteQueuePriorities: u32,
    pub pointSizeRange: [f32; 2],
    pub lineWidthRange: [f32; 2],
    pub pointSizeGranularity: f32,
    pub lineWidthGranularity: f32,
    pub strictLines: u32,
    pub standardSampleLocations: u32,
    pub optimalBufferCopyOffsetAlignment: u64,
    pub optimalBufferCopyRowPitchAlignment: u64,
    pub nonCoherentAtomSize: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPhysicalDeviceSparseProperties {
    pub residencyStandard2DBlockShape: u32,
    pub residencyStandard2DMultisampleBlockShape: u32,
    pub residencyStandard3DBlockShape: u32,
    pub residencyAlignedMipSize: u32,
    pub residencyNonResidentStrict: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPhysicalDeviceProperties {
    pub apiVersion: u32,
    pub driverVersion: u32,
    pub vendorID: u32,
    pub deviceID: u32,
    pub deviceType: u32,
    pub deviceName: [c_char; 256],
    pub pipelineCacheUUID: [u8; 16],
    pub limits: VkPhysicalDeviceLimits,
    pub sparseProperties: VkPhysicalDeviceSparseProperties,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPhysicalDeviceMemoryProperties {
    pub memoryTypeCount: u32,
    pub memoryTypes: [VkMemoryType; 32],
    pub memoryHeapCount: u32,
    pub memoryHeaps: [VkMemoryHeap; 16],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkMemoryType {
    pub propertyFlags: u32,
    pub heapIndex: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkMemoryHeap {
    pub size: u64,
    pub flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkBufferCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub size: u64,
    pub usage: u32,
    pub sharingMode: u32,
    pub queueFamilyIndexCount: u32,
    pub pQueueFamilyIndices: *const u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkMemoryRequirements {
    pub size: u64,
    pub alignment: u64,
    pub memoryTypeBits: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkMemoryAllocateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub allocationSize: u64,
    pub memoryTypeIndex: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkMappedMemoryRange {
    pub sType: u32,
    pub pNext: *const c_void,
    pub memory: VkDeviceMemory,
    pub offset: u64,
    pub size: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkDescriptorSetLayoutBinding {
    pub binding: u32,
    pub descriptorType: u32,
    pub descriptorCount: u32,
    pub stageFlags: u32,
    pub pImmutableSamplers: *const VkSampler,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkDescriptorSetLayoutCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub bindingCount: u32,
    pub pBindings: *const VkDescriptorSetLayoutBinding,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPushConstantRange {
    pub stageFlags: u32,
    pub offset: u32,
    pub size: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPipelineLayoutCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub setLayoutCount: u32,
    pub pSetLayouts: *const VkDescriptorSetLayout,
    pub pushConstantRangeCount: u32,
    pub pPushConstantRanges: *const VkPushConstantRange,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkShaderModuleCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub codeSize: usize,
    pub pCode: *const u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPipelineShaderStageCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub stage: u32,
    pub module: VkShaderModule,
    pub pName: *const c_char,
    pub pSpecializationInfo: *const c_void,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkComputePipelineCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub stage: VkPipelineShaderStageCreateInfo,
    pub layout: VkPipelineLayout,
    pub basePipelineHandle: VkPipeline,
    pub basePipelineIndex: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkDescriptorPoolSize {
    pub r#type: u32,
    pub descriptorCount: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkDescriptorPoolCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub maxSets: u32,
    pub poolSizeCount: u32,
    pub pPoolSizes: *const VkDescriptorPoolSize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkDescriptorSetAllocateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub descriptorPool: VkDescriptorPool,
    pub descriptorSetCount: u32,
    pub pSetLayouts: *const VkDescriptorSetLayout,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkDescriptorBufferInfo {
    pub buffer: VkBuffer,
    pub offset: u64,
    pub range: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkWriteDescriptorSet {
    pub sType: u32,
    pub pNext: *const c_void,
    pub dstSet: VkDescriptorSet,
    pub dstBinding: u32,
    pub dstArrayElement: u32,
    pub descriptorCount: u32,
    pub descriptorType: u32,
    pub pImageInfo: *const c_void,
    pub pBufferInfo: *const VkDescriptorBufferInfo,
    pub pTexelBufferView: *const VkBufferView,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkCommandPoolCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub queueFamilyIndex: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkCommandBufferAllocateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub commandPool: VkCommandPool,
    pub level: u32,
    pub commandBufferCount: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkCommandBufferBeginInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
    pub pInheritanceInfo: *const c_void,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkBufferCopy {
    pub srcOffset: u64,
    pub dstOffset: u64,
    pub size: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkMemoryBarrier {
    pub sType: u32,
    pub pNext: *const c_void,
    pub srcAccessMask: u32,
    pub dstAccessMask: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkFenceCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkSubmitInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub waitSemaphoreCount: u32,
    pub pWaitSemaphores: *const VkSemaphore,
    pub pWaitDstStageMask: *const u32,
    pub commandBufferCount: u32,
    pub pCommandBuffers: *const VkCommandBuffer,
    pub signalSemaphoreCount: u32,
    pub pSignalSemaphores: *const VkSemaphore,
}

#[repr(C)]
pub struct VkPhysicalDeviceProperties2 {
    pub kind: u32,
    pub next: *mut c_void,
    pub base: VkPhysicalDeviceProperties,
}

#[repr(C)]
pub struct VkPhysicalDeviceFloatControlsProperties {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub denormBehaviorIndependence: u32,
    pub roundingModeIndependence: u32,
    pub shaderSignedZeroInfNanPreserveFloat16: u32,
    pub shaderSignedZeroInfNanPreserveFloat32: u32,
    pub shaderSignedZeroInfNanPreserveFloat64: u32,
    pub shaderDenormPreserveFloat16: u32,
    pub shaderDenormPreserveFloat32: u32,
    pub shaderDenormPreserveFloat64: u32,
    pub shaderDenormFlushToZeroFloat16: u32,
    pub shaderDenormFlushToZeroFloat32: u32,
    pub shaderDenormFlushToZeroFloat64: u32,
    pub shaderRoundingModeRTEFloat16: u32,
    pub shaderRoundingModeRTEFloat32: u32,
    pub shaderRoundingModeRTEFloat64: u32,
    pub shaderRoundingModeRTZFloat16: u32,
    pub shaderRoundingModeRTZFloat32: u32,
    pub shaderRoundingModeRTZFloat64: u32,
}

#[repr(C)]
pub struct VkPhysicalDeviceSubgroupProperties {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub size: u32,
    pub stages: u32,
    pub operations: u32,
    pub quad: u32,
}
