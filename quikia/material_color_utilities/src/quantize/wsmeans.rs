use std::cmp::Ordering;
use std::collections::HashMap;
use rand::Rng;
use crate::quantize::lab::Lab;
use crate::utils::Argb;

const MAX_ITERATIONS: usize = 100;
const MIN_DELTA_E: f64 = 3.0;

#[derive(Clone, Default,Debug)]
pub struct QuantizerResult {
    pub color_to_count: HashMap<Argb, usize>,
    pub input_pixel_to_cluster_pixel: HashMap<Argb, Argb>,
}

#[derive(Clone, Copy, Default)]
struct Swatch {
    argb: Argb,
    population: usize,
}

impl Eq for Swatch {}

impl PartialEq<Self> for Swatch {
    fn eq(&self, other: &Self) -> bool {
        self.population == other.population && self.argb == other.argb
    }
}

impl PartialOrd<Self> for Swatch {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.population.partial_cmp(&other.population)
    }
}

impl Ord for Swatch {
    fn cmp(&self, other: &Self) -> Ordering {
        self.population.cmp(&other.population)
    }
}

#[derive(Clone, Copy, Default)]
struct DistanceToIndex {
    distance: f64,
    index: usize,
}

impl Eq for DistanceToIndex {}

impl PartialEq<Self> for DistanceToIndex {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance && self.index == other.index
    }
}

impl PartialOrd<Self> for DistanceToIndex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.distance.partial_cmp(&other.distance)
    }
}

impl Ord for DistanceToIndex {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

pub fn quantize_wsmeans(input_pixels: &[Argb], starting_clusters: &[Argb], max_colors: u16) -> QuantizerResult {
    let mut max_colors = max_colors;
    if max_colors == 0 || input_pixels.is_empty() {
        return QuantizerResult::default();
    }

    if max_colors > 256 {
        // If colors is outside the range, just set it the max.
        max_colors = 256;
    }

    let pixel_count = input_pixels.len();
    let mut pixel_to_count: HashMap<Argb, usize> = HashMap::new();
    let mut pixels: Vec<Argb> = Vec::with_capacity(pixel_count);
    let mut points: Vec<Lab> = Vec::with_capacity(pixel_count);

    for pixel in input_pixels {
        if let Some(count) = pixel_to_count.get_mut(pixel) {
            *count += 1;
        } else {
            pixels.push(*pixel);
            points.push(Lab::from(*pixel));
            pixel_to_count.insert(*pixel, 1);
        }
    }

    let mut cluster_count = points.len().min(max_colors as usize);

    if !starting_clusters.is_empty() {
        cluster_count = starting_clusters.len().min(max_colors as usize);
    }

    let mut pixel_count_sums: [usize; 256] = [0; 256];
    let mut clusters: Vec<Lab> = Vec::with_capacity(starting_clusters.len());
    for argb in starting_clusters {
        clusters.push(Lab::from(*argb));
    }

    let mut rng = rand::thread_rng();
    let additional_clusters_needed = cluster_count - clusters.len();
    if starting_clusters.is_empty() && additional_clusters_needed > 0 {
        for _i in 0..additional_clusters_needed {
            let l = rng.gen::<f64>() * 100.0;
            let a = rng.gen::<f64>() * 200.0 - 100.0;
            let b = rng.gen::<f64>() * 200.0 - 100.0;
            clusters.push(Lab::new(l, a, b));
        }
    }

    let mut cluster_indices: Vec<usize> = Vec::with_capacity(points.len());
    for _i in 0..points.len() {
        cluster_indices.push(rng.gen_range(0..cluster_count));
    }

    let mut index_matrix: Vec<Vec<usize>> = vec![vec![0; cluster_count]; cluster_count];

    let mut distance_to_index_matrix: Vec<Vec<DistanceToIndex>> = vec![vec![DistanceToIndex::default(); cluster_count]; cluster_count];

    for iteration in 0..MAX_ITERATIONS {
        // Calculate cluster distances
        for i in 0..cluster_count {
            distance_to_index_matrix[i][i].distance = 0.0;
            distance_to_index_matrix[i][i].index = i;
            for j in i + 1..cluster_count {
                let distance = clusters[i].delta_e(clusters[j]);

                distance_to_index_matrix[j][i].distance = distance;
                distance_to_index_matrix[j][i].index = i;
                distance_to_index_matrix[i][j].distance = distance;
                distance_to_index_matrix[i][j].index = j;
            }

            let row = &mut distance_to_index_matrix[i];
            row.sort();

            (0..cluster_count).for_each(|j|{
                index_matrix[i][j] = row[j].index;
            });
        }

        // Reassign points
        let mut color_moved = false;
        for i in 0..points.len() {
            let point = points[i];

            let previous_cluster_index = cluster_indices[i];
            let previous_cluster = clusters[previous_cluster_index];
            let previous_distance = point.delta_e(previous_cluster);
            let mut minimum_distance = previous_distance;
            let mut new_cluster_index = -1;

            for j in 0..cluster_count {
                if distance_to_index_matrix[previous_cluster_index][j].distance >= 4.0 * previous_distance {
                    continue;
                }
                let distance = point.delta_e(clusters[j]);
                if distance < minimum_distance {
                    minimum_distance = distance;
                    new_cluster_index = j as isize;
                }
            }

            if new_cluster_index != -1 {
                let distance_change = (minimum_distance.sqrt() - previous_distance.sqrt()).abs();
                if distance_change > MIN_DELTA_E {
                    color_moved = true;
                    cluster_indices[i] = new_cluster_index as usize;
                }
            }

        }

        if !color_moved && (iteration != 0) {
            break;
        }

        // Recalculate cluster centers
        let mut component_a_sums: [f64; 256] = [0.0; 256];
        let mut component_b_sums: [f64; 256] = [0.0; 256];
        let mut component_c_sums: [f64; 256] = [0.0; 256];

        for i in 0..points.len() {
            let cluster_index = cluster_indices[i];
            let point = points[i];
            let count = pixel_to_count[&pixels[i]];

            pixel_count_sums[cluster_index] += count;
            component_a_sums[cluster_index] += point.l * count as f64;
            component_b_sums[cluster_index] += point.a * count as f64;
            component_c_sums[cluster_index] += point.b * count as f64;
        }

        for i in 0..cluster_count {
            let count = pixel_count_sums[i];
            if count == 0 {
                clusters[i] = Lab::default();
                continue;
            }
            let a = component_a_sums[i] / count as f64;
            let b = component_b_sums[i] / count as f64;
            let c = component_c_sums[i] / count as f64;
            clusters[i] = Lab::new(a, b, c);
        }
    }

    let mut swatches: Vec<Swatch> = Vec::new();
    let mut cluster_argbs: Vec<Argb> = Vec::new();
    let mut all_cluster_argbs: Vec<Argb> = Vec::new();
    for i in 0..cluster_count {
        let possible_new_cluster = clusters[i].to_argb();
        all_cluster_argbs.push(possible_new_cluster);

        let count = pixel_count_sums[i];

        if count == 0 {
            continue;
        }
        let mut use_new_cluster = 1;
        for j in 0..swatches.len() {
            if swatches[j].argb == possible_new_cluster {
                swatches[j].population += count;
                use_new_cluster = 0;
                break;
            }
        }

        println!("use_new_cluster: {}", use_new_cluster);
        if use_new_cluster == 0 {
            continue;
        }
        cluster_argbs.push(possible_new_cluster);
        swatches.push(Swatch { argb: possible_new_cluster, population: count });
    }
    swatches.sort();

// Constructs the quantizer result to return.

    let mut color_to_count: HashMap<Argb, usize> = HashMap::new();
    for i in 0..swatches.len() {
        color_to_count.insert(swatches[i].argb, swatches[i].population);
    }

    let mut input_pixel_to_cluster_pixel: HashMap<Argb, Argb> = HashMap::new();
    for i in 0..points.len() {
        let pixel = pixels[i];
        let cluster_index = cluster_indices[i];
        let cluster_argb = all_cluster_argbs[cluster_index];
        input_pixel_to_cluster_pixel.insert(pixel, cluster_argb);
    }

    QuantizerResult {
        color_to_count,
        input_pixel_to_cluster_pixel,
    }
}