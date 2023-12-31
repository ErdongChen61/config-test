use base64::{engine::general_purpose::STANDARD as BS64, Engine};
use ff::PrimeField;
use halo2_proofs::{
    plonk::{self, create_proof, keygen_pk, keygen_vk, verify_proof},
    poly::kzg::{
        commitment::{KZGCommitmentScheme, ParamsKZG},
        multiopen::{ProverGWC, VerifierGWC},
        strategy::SingleStrategy,
    },
    transcript::{
        Blake2bRead, Blake2bWrite, Challenge255, TranscriptReadBuffer, TranscriptWriterBuffer,
    },
};
use halo2curves::bn256::{Bn256, Fr, G1Affine};
use poseidon_circuit::test_circuit;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use snarkify_sdk::prover::ProofHandler;
use std::env;
use std::fs;
use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::time::Instant;

/// A prover for Poseidon hashes using the Halo2 proving system.
struct PoseidonProver;

/// Represents the inputs to the Poseidon Circuit
///
/// This struct is designed to capture the necessary inputs for the
/// Poseidon hash circuit.
#[derive(Deserialize)]
pub struct Input {
    /// The private_input vector, representing the hash input
    ///
    /// These inputs are part of the witness
    private_input: Vec<u64>,

    /// The public_input string, representing the hash output
    ///
    /// This is the expected Poseidon hash value of [`Self::private_input`]
    public_input: String,
}

impl Input {
    /// Converts the private input vector of [`u64`] to a vector of [`Fp`]
    pub fn private_input(&self) -> Vec<Fr> {
        self.private_input
            .iter()
            .copied()
            .map(Fr::from)
            .collect::<Vec<_>>()
    }

    /// Parses the public input from a string to `Fp`
    pub fn public_input(&self) -> Result<Fr, Error> {
        Fr::from_str_vartime(&self.public_input).ok_or_else(|| Error::PubInputOutOfField {
            public_input: self.public_input.clone(),
        })
    }
}

fn list_dir_recursive(path: &Path) {
    if path.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_file() {
                        println!("File: {:?}", path.display());
                    } else if path.is_dir() {
                        println!("Directory: {:?}", path.display());
                        list_dir_recursive(&path);
                    }
                }
            }
        }
    }
}

fn list_dirs(path: &Path) {
    if path.is_dir() {
        match fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.is_dir() {
                            println!("Directory: {:?}", path.display());
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to read directory {:?}: {}", path.display(), e);
            }
        }
    }
}


impl ProofHandler for PoseidonProver {
    type Input = Input;
    type Output = String;
    type Error = Error;

    /// Generates a zk-SNARK proof for the Poseidon hash function.
    ///
    /// Given an [`Input`] instance containing the private and public inputs,
    /// this function goes through the steps of setting up the proving parameters,
    /// generating a proof, and then verifying that proof, ultimately returning
    /// a serialized proof in the form of a Base64-encoded string.
    ///
    /// # Arguments
    ///
    /// * `input` - An `Input` struct containing:
    ///   - `private_input`: A `Vec<u64>` representing the private part of the input to the hash function.
    ///   - `public_input`: A `String` representing the expected hash output in the field `Fp`.
    ///
    /// # Returns
    ///
    /// If successful, it returns `Ok(String)` where the string is the Base64-encoded
    /// representation of the generated zk-SNARK proof. If any step in the proof generation
    /// or verification fails, it returns an `Err(Error)`, which captures and conveys
    /// the specific stage and nature of the failure.
    fn prove(input: Self::Input) -> Result<Self::Output, Self::Error> {
        // Read the YAML file into a String.
        // Get the current directory
        match env::current_dir() {
            Ok(current_dir) => {
                println!("Current directory: {:?}", current_dir);
                list_dir_recursive(&current_dir);
            }
            Err(e) => {
                eprintln!("Failed to get the current directory: {}", e);
            }
        }
        match env::var("x") {
            Ok(value) => println!("The value of x is: {}", value),
            Err(e) => println!("Couldn't read x ({})", e),
        }
        println!("Root dir");
        let root = Path::new("/");
        list_dirs(root);
        let yaml_str = std::fs::read_to_string("src/configs/config.yaml").expect("Failed to read config file");
        
        // Deserialize the YAML string into your Config struct.
        let config: Config = serde_yaml::from_str(&yaml_str).expect("Failed to parse YAML");

        // Now you can access the configuration values as needed.
        println!("{:?}", config);

        // Attempt to read the entire file
        let start = Instant::now(); // Start the timer
        let file_path = Path::new("/mnt/data/kzg_bn254_15.srs");
        let mut file_contents = Vec::new();

        match File::open(&file_path) {
            Ok(mut file) => {
                if let Err(e) = file.read_to_end(&mut file_contents) {
                    eprintln!("Failed to read the file: {}", e);
                } else {
                    let duration = start.elapsed(); // Calculate the duration
                    println!("Time taken to read file: {:?}", duration);
                }
            }
            Err(e) => eprintln!("Failed to open the file: {}", e),
        };

        // The security parameter `k` for the construction, affecting the size and security of the proving system.
        const K: u32 = 10;

        let params = ParamsKZG::<Bn256>::setup(K, OsRng);

        let private_inputs = input.private_input();
        let circuit = test_circuit::TestCircuit::new(private_inputs);

        let vk = keygen_vk(&params, &circuit).map_err(Error::while_keygen_vk)?;
        let pk = keygen_pk(&params, vk, &circuit).map_err(Error::while_keygen_pk)?;

        let out_hash = input.public_input()?;
        let public_inputs: &[&[Fr]] = &[&[out_hash]];

        // Initialize the proof transcript with a Blake2b hash function.
        let mut proof_transcript = Blake2bWrite::<_, G1Affine, Challenge255<_>>::init(vec![]);

        // Create the zk-SNARK proof for the circuit and public inputs.
        create_proof::<KZGCommitmentScheme<_>, ProverGWC<'_, _>, _, _, _, _>(
            &params,
            &pk,
            &[circuit],
            &[public_inputs],
            OsRng,
            &mut proof_transcript,
        )
        .map_err(Error::while_prove)?;
        let proof = proof_transcript.finalize();

        // Verify the proof to ensure its correctness before sending it off.
        let mut verify_transcript = Blake2bRead::<_, _, Challenge255<_>>::init(&proof[..]);
        let strategy = SingleStrategy::new(&params);
        verify_proof::<
            KZGCommitmentScheme<Bn256>,
            VerifierGWC<'_, Bn256>,
            Challenge255<G1Affine>,
            Blake2bRead<&[u8], G1Affine, Challenge255<G1Affine>>,
            SingleStrategy<'_, Bn256>,
        >(
            &params,
            pk.get_vk(),
            strategy,
            &[public_inputs],
            &mut verify_transcript,
        )
        .map_err(Error::while_verify)?;

        Ok(BS64.encode(proof))
    }
}

/// Enumerates the potential errors that can occur within the [`PoseidonProver`].
///
/// This error enum captures the various points of failure that could occur
/// during the setup, proof generation, and verification steps of the Poseidon
/// proving process.
///
/// Note: The [`plonk::Error`] type is not serializable, hence we convert it to a string
/// to capture the error information. This workaround allows us to include `plonk::Error`
/// information in a serializable format.
#[derive(Serialize)]
pub enum Error {
    WhileKeygenVk { plonk_error: String },
    WhileKeygenPk { plonk_error: String },
    PubInputOutOfField { public_input: String },
    WhileProve { plonk_error: String },
    WhileVerify { plonk_error: String },
}

impl Error {
    fn while_keygen_vk(err: plonk::Error) -> Self {
        Self::WhileKeygenVk {
            plonk_error: format!("{err:?}"),
        }
    }
    fn while_keygen_pk(err: plonk::Error) -> Self {
        Self::WhileKeygenPk {
            plonk_error: format!("{err:?}"),
        }
    }
    fn while_prove(err: plonk::Error) -> Self {
        Self::WhileProve {
            plonk_error: format!("{err:?}"),
        }
    }
    fn while_verify(err: plonk::Error) -> Self {
        Self::WhileProve {
            plonk_error: format!("{err:?}"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Config {
    x: String,
}

fn main() -> Result<(), std::io::Error> {
    snarkify_sdk::run::<PoseidonProver>()
}
