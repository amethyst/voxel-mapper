<img src="screenshots/splash.png" alt="Amethyst Game Engine" />

![Pic](/screenshots/blending.png)

The Voxel Mapper is a set of Amethyst-compatible systems for creating beautiful
voxel worlds.

![Demo](/screenshots/demo.gif)

Supports both smooth voxels and cube voxels. Just edit the `MeshMode` resource.

![Demo](/screenshots/cubey.PNG)

## Editor Usage

To build and run with the example assets:

```
run/download_assets.sh

GRAPHICS_BACKEND=metal
cargo run --bin editor --release --features amethyst/$GRAPHICS_BACKEND,amethyst/no-slow-safety-checks -- assets/maps/example_map.ron
```

When you exit the app, a binary file "saved_voxels.bin" will contain the map you just created.
You can load it back into the editor by setting `voxels_file_path: Some("saved_voxels.bin")` in "assets/maps/example_map.ron."

Control bindings can be found in "assets/config/map_editor_bindings.ron".

If you want to import your own material images, take a look at [material-converter](https://github.com/bonsairobo/material-converter).
It makes it easy to import material images from sites like freepbr.com (don't you wish they meant the beer?).

## Library Usage

To use the voxel mapper in your own Amethyst app, you'll need to:

- Add the `VoxelSystemBundle` to your `Dispatcher`
- Add the `RenderSplattedTriplanarPbr` render plugin to your renderer
- Insert a `VoxelMap` into your `World`
    - You can create one in the editor and save it to a ".bin" file
    - Reference the ".bin" file in your RON map file and load it with `load_voxel_map`
- Insert a `VoxelAssets` into your `World`
    - You load the assets using the `VoxelAssetLoader` and your `VoxelMap`

## Development

It's early days for this project. These features are currently supported:

- a voxel map that supports:
    - real-time editing (terraforming)
    - serialization
    - real-time LZ4-(de)compression
    - LRU-caching of decompressed chunks
- real-time, smooth chunk meshing using Surface Nets
- multiple blended materials (texture splatting)
- physically-based, triplanar material rendering, courtesy of Amethyst
- a mouse-based terraforming controller
- a camera controller that resolves collisions with the voxels

Planned features (by priority):

1. multiple array materials
2. more realistic texture splatting using depth textures
3. shadows
4. procedural generation
5. streaming voxel maps from persistent storage
6. dynamic voxel types (e.g. water, foliage)
7. GPU acceleration of voxel algorithms
8. beautiful example maps
9. level of detail
10. texture detiling
