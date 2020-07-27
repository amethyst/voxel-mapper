use voxel_mapper::{
    assets::{read_bincode_file, BincodeFileError},
    voxel::Voxel,
};

use ilattice3::{ChunkedLatticeMap, Extent};
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

    let voxels: ChunkedLatticeMap<Voxel> = read_bincode_file(opt.map_file)?;

    let min = [opt.x, opt.y, opt.z];
    let size = [opt.sx, opt.sy, opt.sz];
    let dump_extent = Extent::from_min_and_local_supremum(min.into(), size.into());
    println!("extent = {:?}", dump_extent);

    for (p, voxel) in voxels.iter_point_values(dump_extent) {
        println!("{:?} {:?}", p, voxel);
    }

    Ok(())
}
