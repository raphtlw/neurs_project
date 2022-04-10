/*!
 * An amorphous method of training a neural network.
 *
 * The method works by considering an example from the training set, and
 * testing the neural network on it multiple times, slightly 'jittering'(!) the
 * weights and biases of the network each time; after a certain desired number
 * of attempts, the current network weights are adjusted towards the best
 * performing variations.
 *
 * 'Amorphous' here means that the method itself could, in its general
 * form, apply to any set of parameters which can be measured with fitness.
 * In other words, it can be applied to a much more general case than neural
 * networks. However, the implementation provided here is specific to neural
 * networks, for the sake of performance and code simplicity.
 */
use super::super::neuralnet::SimpleNeuralNetwork;
use super::interface::{TrainingFrame, TrainingStrategy};
use crate::neuralnet::NeuralLayer;
use rand::thread_rng;
use rand_distr::*;

/**
 * The weight-jitter training strategy.
 */
pub struct WeightJitterStrat {
    /// How many different 'jitters' of the same weight should be tried.
    pub num_jitters: usize,

    /// Whether bad jitters should be taken into account when adjusting the
    /// current network's weights (by "moving away from" them).
    pub apply_bad_jitters: bool,

    /// How much the weights should be randomized in a jitter.
    pub jitter_width: f32,

    /// The amount of jitter_width that should be culled away with each epoch.
    pub jitter_width_falloff: f32,

    /// How much the weights should be adjusted after an epoch.
    pub step_factor: f32,

    /// How many cycles of compute and get-fitness should be run per network,
    /// per epoch.
    pub num_steps_per_epoch: usize,

    /* Internals. */
    pub curr_jitter_width: f32,
}

pub struct WeightJitterStratOptions {
    /// How many different 'jitters' of the same weight should be tried.
    pub num_jitters: usize,

    /// Whether bad jitters should be taken into account when adjusting the
    /// current network's weights (by "moving away from" them).
    pub apply_bad_jitters: bool,

    /// How much the weights should be randomized in a jitter.
    pub jitter_width: f32,

    /// The amount of jitter_width that should be culled away with each epoch.
    pub jitter_width_falloff: f32,

    /// How much the weights should be adjusted after an epoch.
    pub step_factor: f32,

    /// How many cycles of compute and get-fitness should be run per network,
    /// per epoch.
    pub num_steps_per_epoch: usize,
}

impl WeightJitterStrat {
    pub fn new(options: WeightJitterStratOptions) -> WeightJitterStrat {
        WeightJitterStrat {
            num_jitters: options.num_jitters,
            jitter_width: options.jitter_width,
            jitter_width_falloff: options.jitter_width_falloff,
            step_factor: options.step_factor,
            num_steps_per_epoch: options.num_steps_per_epoch,
            apply_bad_jitters: options.apply_bad_jitters,

            curr_jitter_width: options.jitter_width,
        }
    }
}

fn jitter_values<D: Distribution<f32>>(values: &mut [f32], distrib: D) {
    for value in values {
        *value += distrib.sample(&mut thread_rng());
    }
}

#[derive(Clone)]
struct WeightsAndBiases {
    w: Vec<f32>,
    b: Vec<f32>,
}

#[allow(unused)]
impl WeightsAndBiases {
    fn zero(&mut self) {
        self.w.fill(0.0);
        self.b.fill(0.0);
    }

    fn jitter<D: Distribution<f32>>(&mut self, distrib: &D) {
        jitter_values(&mut self.w, &distrib);
        jitter_values(&mut self.b, &distrib);
    }

    fn apply_to(&self, dest_layer: &mut NeuralLayer) {
        if cfg!(dbg) {
            assert!(dest_layer.weights.len() == self.w.len());
            assert!(dest_layer.biases.len() == self.b.len());
        }

        dest_layer.weights.clone_from(&self.w);
        dest_layer.biases.clone_from(&self.b);
    }

    fn scale(&mut self, scale: f32) {
        for w in &mut self.w {
            *w *= scale;
        }

        for b in &mut self.b {
            *b *= scale;
        }
    }

    fn scale_from(&mut self, other: &WeightsAndBiases, scale: f32) {
        for (i, ow) in other.w.iter().enumerate() {
            let w = self.w[i];
            let diff = w - ow;

            self.w[i] += diff * scale - diff;
        }

        for (i, ob) in other.b.iter().enumerate() {
            let b = self.b[i];
            let diff = b - ob;

            self.b[i] += diff * scale - diff;
        }
    }

    fn sub_from(&mut self, other: &WeightsAndBiases) {
        for (i, ow) in other.w.iter().enumerate() {
            self.w[i] -= ow;
        }

        for (i, ob) in other.b.iter().enumerate() {
            self.b[i] -= ob;
        }
    }

    fn add_to(&self, other: &mut WeightsAndBiases) {
        for (i, w) in self.w.iter().enumerate() {
            other.w[i] += w;
        }

        for (i, b) in self.b.iter().enumerate() {
            other.b[i] += b;
        }
    }
}

impl From<&NeuralLayer> for WeightsAndBiases {
    fn from(src_layer: &NeuralLayer) -> WeightsAndBiases {
        WeightsAndBiases {
            w: src_layer.weights.clone(),
            b: src_layer.biases.clone(),
        }
    }
}

impl From<&mut NeuralLayer> for WeightsAndBiases {
    fn from(src_layer: &mut NeuralLayer) -> WeightsAndBiases {
        WeightsAndBiases {
            w: src_layer.weights.clone(),
            b: src_layer.biases.clone(),
        }
    }
}

#[derive(Clone)]
struct WnbList {
    wnbs: Vec<WeightsAndBiases>,
}

#[allow(unused)]
impl WnbList {
    fn zero(&mut self) {
        for wnb in &mut self.wnbs {
            wnb.zero()
        }
    }

    fn apply_to(&self, dest_net: &mut SimpleNeuralNetwork) {
        if cfg!(dbg) {
            assert!(dest_net.layers.len() == self.wnbs.len());
        }

        for (i, wnb) in self.wnbs.iter().enumerate() {
            wnb.apply_to(&mut dest_net.layers[i]);
        }
    }

    fn jitter<D: Distribution<f32>>(&mut self, distrib: &D) {
        for wnb in &mut self.wnbs {
            wnb.jitter(&distrib);
        }
    }

    fn scale(&mut self, scale: f32) {
        for wnb in &mut self.wnbs {
            wnb.scale(scale);
        }
    }

    fn scale_from(&mut self, other: &WnbList, scale: f32) {
        for (wnb, ownb) in self.wnbs.iter_mut().zip(&other.wnbs) {
            wnb.scale_from(ownb, scale);
        }
    }

    fn add_to(&self, other: &mut WnbList) {
        for (wnb, ownb) in self.wnbs.iter().zip(&mut other.wnbs) {
            wnb.add_to(ownb);
        }
    }

    fn sub_from(&mut self, other: &WnbList) {
        for (wnb, ownb) in self.wnbs.iter_mut().zip(&other.wnbs) {
            wnb.sub_from(ownb);
        }
    }
}

impl From<&SimpleNeuralNetwork> for WnbList {
    fn from(src_net: &SimpleNeuralNetwork) -> WnbList {
        WnbList {
            wnbs: src_net.layers.iter().map(WeightsAndBiases::from).collect(),
        }
    }
}

impl From<&mut SimpleNeuralNetwork> for WnbList {
    fn from(src_net: &mut SimpleNeuralNetwork) -> WnbList {
        WnbList {
            wnbs: src_net.layers.iter().map(WeightsAndBiases::from).collect(),
        }
    }
}

impl TrainingStrategy for WeightJitterStrat {
    fn reset_training(&mut self) {
        self.curr_jitter_width = self.jitter_width;
    }

    fn epoch(
        &mut self,
        net: &mut SimpleNeuralNetwork,
        frame: &mut Box<dyn TrainingFrame>,
    ) -> Result<f32, String> {
        debug_assert!(self.num_jitters > 0);
        debug_assert!(self.jitter_width >= 0.0);
        debug_assert!(self.num_steps_per_epoch > 0);
        debug_assert!(self.step_factor >= 0.0);

        let mut output = vec![0.0; net.output_size()? as usize];

        let mut reference_fitness = 0.0;

        for _ in 0..self.num_steps_per_epoch {
            let reference_input = frame.next_training_case();
            net.compute_values(&reference_input, &mut output)?;
            reference_fitness += frame.get_reference_fitness(&reference_input, &output);
        }

        reference_fitness /= self.num_steps_per_epoch as f32;

        let reference_wnb: WnbList = WnbList::from(&*net);
        let mut new_wnb: WnbList = reference_wnb.clone();
        // new_wnb.zero();

        let distrib = Normal::new(0.0, self.curr_jitter_width).unwrap();
        let mut jitter_results: Vec<(WnbList, f32)> =
            vec![(reference_wnb.clone(), 0.0); self.num_jitters];

        for result in &mut jitter_results {
            result.0.jitter(&distrib);
        }

        // Get fitnesses
        for result in &mut jitter_results {
            result.0.apply_to(net);

            frame.reset_frame();

            for _ in 0..self.num_steps_per_epoch {
                let next_input = frame.next_training_case();
                net.compute_values(&next_input, &mut output)?;

                let fit = frame.get_fitness(&next_input, &output);

                let delta_fit = fit - reference_fitness;
                result.1 += delta_fit;
            }

            result.1 /= self.num_steps_per_epoch as f32;
        }

        // Apply jitters

        let min_fitness = jitter_results
            .iter()
            .map(|x| x.1)
            .reduce(|ac, n| if ac < n { ac } else { n })
            .unwrap();
        let max_fitness = jitter_results
            .iter()
            .map(|x| x.1)
            .reduce(|ac, n| if ac > n { ac } else { n })
            .unwrap();

        let num_ok_jitters = if self.apply_bad_jitters {
            self.num_jitters
        } else {
            jitter_results
                .iter()
                .map(|x| if x.1 > 0.0 { 1_usize } else { 0_usize })
                .sum::<usize>()
        };

        if num_ok_jitters > 0 {
            let step_factor = self.step_factor / num_ok_jitters as f32;

            for (wnbs, fitness) in &mut jitter_results {
                if self.apply_bad_jitters || *fitness > 0.0 {
                    let fitness_scale = (*fitness - min_fitness)
                        / (if max_fitness == min_fitness {
                            1.0
                        } else {
                            max_fitness - min_fitness
                        })
                        * 2.0
                        - 1.0;

                    wnbs.sub_from(&reference_wnb);
                    wnbs.scale(fitness_scale * step_factor);
                    wnbs.add_to(&mut new_wnb);
                }
            }

            //println!("Applied {} jitters.", num_ok_jitters);
        } else {
            new_wnb = reference_wnb.clone();

            //println!("Applied NO jitters.");
        }

        self.curr_jitter_width *= 1.0 - self.jitter_width_falloff;

        new_wnb.apply_to(net);

        Ok(max_fitness + reference_fitness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::train::{label, trainer};
    use crate::{activations, neuralnet};

    #[test]
    fn test_jitter_training() {
        let xor_net = neuralnet::SimpleNeuralNetwork::new_simple_with_activation(
            &[2, 3, 2],
            Some(activations::fast_sigmoid),
        );

        let frame: label::LabeledLearningFrame<usize> = label::LabeledLearningFrame::new(
            vec![
                vec![1.0, 0.0],
                vec![0.0, 1.0],
                vec![1.0, 1.0],
                vec![0.0, 0.0],
            ],
            vec![1, 1, 0, 0],
            Some(Box::new(|x: f32| x * x)),
            true,
        )
        .unwrap();

        let num_cases = frame.num_cases();
        println!("There are {} training cases.", num_cases);

        let strategy = WeightJitterStrat::new(WeightJitterStratOptions {
            apply_bad_jitters: true,
            num_jitters: 100,
            jitter_width: 1.0,
            jitter_width_falloff: 0.02,
            step_factor: 0.6,
            num_steps_per_epoch: num_cases,
        });
        let mut jitter_width = strategy.jitter_width;
        let jitter_width_falloff = strategy.jitter_width_falloff;

        let mut trainer =
            trainer::Trainer::new_from_net(&xor_net, Box::from(frame), Box::from(strategy));

        println!("Trainer initialized successfully!");

        println!("Training xor network...");

        for epoch in 1..=150 {
            let best_fitness = trainer.epoch().unwrap();
            jitter_width *= 1.0 - jitter_width_falloff;
            println!(
                "Epoch {} done! Best fitness {}, jitter width now {}",
                epoch, best_fitness, jitter_width
            );
        }

        println!("Done training! Testing XOR network:");

        let mut outputs: Vec<f32> = vec![0.0, 0.0];

        for inp in vec![[0.0, 1.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0]] {
            trainer
                .reference_net
                .compute_values(&inp, &mut outputs)
                .unwrap();
            println!(
                "[{}, {}] -> {:?} ([{}, {}]) (fitness {})",
                inp[0] as u8,
                inp[1] as u8,
                outputs[1] - outputs[0],
                outputs[0],
                outputs[1],
                trainer.frame.get_fitness(&inp, &outputs)
            );
        }

        println!("Asserting answers make sense...");
        let mut ok_cases = 0;

        for (i, inp) in vec![[0.0, 1.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0]]
            .iter()
            .enumerate()
        {
            trainer
                .reference_net
                .compute_values(inp, &mut outputs)
                .unwrap();

            let makes_sense =
                ((outputs[1] - outputs[0]) > 0.0) == ((inp[0] > 0.5) != (inp[1] > 0.5));

            if makes_sense {
                ok_cases += 1;
                println!("Output in case #{} makes sense.", i + 1);
            } else {
                println!("Output in case #{} does NOT make sense.", i + 1);
            }
        }

        println!("{} out of {} cases make sense.", ok_cases, num_cases);
        assert_eq!(ok_cases, num_cases);

        println!("Yay!");
    }
}
