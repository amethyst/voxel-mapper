use voxel_mapper::{
    assets::{read_bincode_file, BincodeFileError},
    voxel::Voxel,
};

use building_blocks::prelude::*;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "dump-voxels")]
struct Opt {
    #[structopt(parse(from_os_str))]
    map_file: PathBuf,

    #[structopt(long)]
    x: i32,
    #[structopt(long)]
    y: i32,
    #[structopt(long)]
    z: i32,
    #[structopt(long)]
    sx: i32,
    #[structopt(long)]
    sy: i32,
    #[structopt(long)]
    sz: i32,
}

fn main() -> Result<(), BincodeFileError> {
    let opt = Opt::from_args();

    let voxels: ChunkMap3<Voxel> = read_bincode_file(opt.map_file)?;

    let min = [opt.x, opt.y, opt.z];
    let shape = [opt.sx, opt.sy, opt.sz];
    let dump_extent = Extent3i::from_min_and_shape(PointN(min), PointN(shape));
    println!("extent = {:?}", dump_extent);

    voxels.for_each_ref(dump_extent, |p: Point3i, voxel|) {
        println!("{:?} {:?}", p, voxel);
    }

    Ok(())
}
