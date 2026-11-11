# Native plugins

Blink language sources and the single-machine runtime are open source in crates and library. Plugins can be delivered independently, with their own native interfaces and platform artifacts.

| Package | Purpose | Source availability | Current status |
|---|---|---|---|
| [DomainBuild](DomainBuild/README.md) | Distributed compilation | Closed source | Planned |
| [DomainCompute](DomainCompute/README.md) | Distributed native execution | Closed source | Planned |
| [DomainFoundation](DomainFoundation/README.md) | Domain and node hosts, administration and supervision | Closed source | Planned |

Closed plugin directories contain public documentation and released binaries. Platform artifacts use Binaries/<os>-<architecture>. Windows x64 is the current base toolchain target. Domain plugin binaries have not been released.

| Target directory | Native artifacts | Support |
|---|---|---|
| windows-x64 | .dll libraries and .exe programs | Domain packages pending implementation |
| windows-arm64 | Windows ARM64 libraries and programs | Planned |
| linux-x64 | .so libraries and native programs | Planned |
| linux-arm64 | .so libraries and native programs | Planned |
| android-arm64 | .so libraries with the supported application package | Planned |

A plugin is bound through its versioned native interface when loaded. Existing work retains its code and resources until its CPU, GPU, communication and callback consumers finish. A new version receives new work according to its supported interface and state contract.
