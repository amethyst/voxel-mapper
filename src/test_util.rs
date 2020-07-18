use amethyst::core::{
    approx::assert_relative_eq,
    math::{Point3, Vector3},
};
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::FromIterator;

pub fn assert_elements_eq<T: Clone + Debug + Eq + Hash>(v1: &Vec<T>, v2: &Vec<T>) {
    let set1: HashSet<T> = HashSet::from_iter(v1.iter().cloned());
    let set2: HashSet<T> = HashSet::from_iter(v2.iter().cloned());
    assert_eq!(set1, set2);
}

pub fn assert_relative_eq_vec(v1: &Vec<f32>, v2: &Vec<f32>) {
    assert_eq!(v1.len(), v2.len());
    for (x1, x2) in v1.iter().zip(v2.iter()) {
        assert_relative_eq!(x1, x2);
    }
}

pub fn assert_relative_eq_vector3(v1: &Vector3<f32>, v2: &Vector3<f32>) {
    assert_relative_eq!(v1.x, v2.x);
    assert_relative_eq!(v1.y, v2.y);
    assert_relative_eq!(v1.z, v2.z);
}

pub fn assert_relative_eq_point3(p1: &Point3<f32>, p2: &Point3<f32>) {
    assert_relative_eq_vector3(&p1.coords, &p2.coords);
}
