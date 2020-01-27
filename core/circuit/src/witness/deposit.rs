use super::utils::*;

use crate::operation::SignatureData;
use crate::operation::*;
use franklin_crypto::bellman::pairing::ff::{Field, PrimeField};
use franklin_crypto::jubjub::JubjubEngine;
use models::circuit::account::CircuitAccountTree;
use models::circuit::utils::{append_be_fixed_width, le_bit_vector_into_field_element};
use models::node::DepositOp;
use models::params as franklin_constants;
use franklin_crypto::bellman::pairing::bn256::*;

pub struct DepositData {
    pub amount: u128,
    pub token: u32,
    pub account_address: u32,
    pub new_pub_key_hash: Fr,
}
pub struct DepositWitness<E: JubjubEngine> {
    pub before: OperationBranch<E>,
    pub after: OperationBranch<E>,
    pub args: OperationArguments<E>,
    pub before_root: Option<E::Fr>,
    pub after_root: Option<E::Fr>,
    pub tx_type: Option<E::Fr>,
}
impl<E: JubjubEngine> DepositWitness<E> {
    pub fn get_pubdata(&self) -> Vec<bool> {
        let mut pubdata_bits = vec![];
        append_be_fixed_width(
            &mut pubdata_bits,
            &self.tx_type.unwrap(),
            franklin_constants::TX_TYPE_BIT_WIDTH,
        );

        append_be_fixed_width(
            &mut pubdata_bits,
            &self.before.address.unwrap(),
            franklin_constants::ACCOUNT_ID_BIT_WIDTH,
        );
        append_be_fixed_width(
            &mut pubdata_bits,
            &self.before.token.unwrap(),
            franklin_constants::TOKEN_BIT_WIDTH,
        );
        append_be_fixed_width(
            &mut pubdata_bits,
            &self.args.full_amount.unwrap(),
            franklin_constants::BALANCE_BIT_WIDTH,
        );

        append_be_fixed_width(
            &mut pubdata_bits,
            &self.args.new_pub_key_hash.unwrap(),
            franklin_constants::NEW_PUBKEY_HASH_WIDTH,
        );
        //        assert_eq!(pubdata_bits.len(), 37 * 8);
        pubdata_bits.resize(6 * franklin_constants::CHUNK_BIT_WIDTH, false);
        pubdata_bits
    }
    pub fn get_sig_bits(&self) -> Vec<bool> {
        let mut sig_bits = vec![];
        append_be_fixed_width(
            &mut sig_bits,
            &Fr::from_str("1").unwrap(), //Corresponding tx_type
            franklin_constants::TX_TYPE_BIT_WIDTH,
        );
        append_be_fixed_width(
            &mut sig_bits,
            &self.args.new_pub_key_hash.unwrap(),
            franklin_constants::NEW_PUBKEY_HASH_WIDTH,
        );
        append_be_fixed_width(
            &mut sig_bits,
            &self.before.token.unwrap(),
            franklin_constants::TOKEN_BIT_WIDTH,
        );
        append_be_fixed_width(
            &mut sig_bits,
            &self.args.full_amount.unwrap(),
            franklin_constants::BALANCE_BIT_WIDTH,
        );
        append_be_fixed_width(
            &mut sig_bits,
            &self.before.witness.account_witness.nonce.unwrap(),
            franklin_constants::NONCE_BIT_WIDTH,
        );
        sig_bits
    }
}

pub fn apply_deposit_tx(
    tree: &mut CircuitAccountTree,
    deposit: &DepositOp,
) -> DepositWitness<Bn256> {
    let alt_new_pubkey_hash = deposit.priority_op.account.to_fr();
    let deposit_data = DepositData {
        amount: deposit.priority_op.amount.to_string().parse().unwrap(),
        token: u32::from(deposit.priority_op.token),
        account_address: deposit.account_id,
        new_pub_key_hash: alt_new_pubkey_hash,
    };
    apply_deposit(tree, &deposit_data)
}
pub fn apply_deposit(
    tree: &mut CircuitAccountTree,
    deposit: &DepositData,
) -> DepositWitness<Bn256> {
    //preparing data and base witness
    let before_root = tree.root_hash();
    println!("deposit Initial root = {}", before_root);
    let (audit_path_before, audit_balance_path_before) =
        get_audits(tree, deposit.account_address, deposit.token);

    let capacity = tree.capacity();
    assert_eq!(capacity, 1 << franklin_constants::account_tree_depth());
    let account_address_fe = Fr::from_str(&deposit.account_address.to_string()).unwrap();
    let token_fe = Fr::from_str(&deposit.token.to_string()).unwrap();
    let amount_as_field_element = Fr::from_str(&deposit.amount.to_string()).unwrap();
    println!("amount_as_field_element is: {}", amount_as_field_element);
    //calculate a and b
    let a = amount_as_field_element;
    let b = Fr::zero();

    //applying deposit
    let (account_witness_before, account_witness_after, balance_before, balance_after) =
        apply_leaf_operation(
            tree,
            deposit.account_address,
            deposit.token,
            |acc| {
                assert!(
                    (acc.pub_key_hash == deposit.new_pub_key_hash)
                        || (acc.pub_key_hash == Fr::zero())
                );
                acc.pub_key_hash = deposit.new_pub_key_hash;
            },
            |bal| bal.value.add_assign(&amount_as_field_element),
        );

    let after_root = tree.root_hash();
    println!("deposit After root = {}", after_root);
    let (audit_path_after, audit_balance_path_after) =
        get_audits(tree, deposit.account_address, deposit.token);

    DepositWitness {
        before: OperationBranch {
            address: Some(account_address_fe),
            token: Some(token_fe),
            witness: OperationBranchWitness {
                account_witness: account_witness_before,
                account_path: audit_path_before,
                balance_value: Some(balance_before),
                balance_subtree_path: audit_balance_path_before,
            },
        },
        after: OperationBranch {
            address: Some(account_address_fe),
            token: Some(token_fe),
            witness: OperationBranchWitness {
                account_witness: account_witness_after,
                account_path: audit_path_after,
                balance_value: Some(balance_after),
                balance_subtree_path: audit_balance_path_after,
            },
        },
        args: OperationArguments {
            ethereum_key: Some(Fr::zero()),
            amount_packed: Some(Fr::zero()),
            full_amount: Some(amount_as_field_element),
            fee: Some(Fr::zero()),
            a: Some(a),
            b: Some(b),
            pub_nonce: Some(Fr::zero()),
            new_pub_key_hash: Some(deposit.new_pub_key_hash),
        },
        before_root: Some(before_root),
        after_root: Some(after_root),
        tx_type: Some(Fr::from_str("1").unwrap()),
    }
}

pub fn calculate_deposit_operations_from_witness(
    deposit_witness: &DepositWitness<Bn256>,
    first_sig_msg: &Fr,
    second_sig_msg: &Fr,
    third_sig_msg: &Fr,
    signature_data: &SignatureData,
    signer_pub_key_packed: &[Option<bool>],
) -> Vec<Operation<Bn256>> {
    let pubdata_chunks: Vec<_> = deposit_witness
        .get_pubdata()
        .chunks(64)
        .map(|x| le_bit_vector_into_field_element(&x.to_vec()))
        .collect();

    println!(
        "acc_path{} \n bal_path {} ",
        deposit_witness.before.witness.account_path.len(),
        deposit_witness.before.witness.balance_subtree_path.len()
    );
    let operation_zero = Operation {
        new_root: deposit_witness.after_root,
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("0").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[0]),
        first_sig_msg: Some(*first_sig_msg),
        second_sig_msg: Some(*second_sig_msg),
        third_sig_msg: Some(*third_sig_msg),
        signature_data: signature_data.clone(),
        signer_pub_key_packed: signer_pub_key_packed.to_vec(),
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.before.clone(),
        rhs: deposit_witness.before.clone(),
    };

    let operation_one = Operation {
        new_root: deposit_witness.after_root,
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("1").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[1]),
        first_sig_msg: Some(*first_sig_msg),
        second_sig_msg: Some(*second_sig_msg),
        third_sig_msg: Some(*third_sig_msg),
        signature_data: signature_data.clone(),
        signer_pub_key_packed: signer_pub_key_packed.to_vec(),
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.after.clone(),
        rhs: deposit_witness.after.clone(),
    };

    let operation_two = Operation {
        new_root: deposit_witness.after_root,
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("2").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[2]),
        first_sig_msg: Some(*first_sig_msg),
        second_sig_msg: Some(*second_sig_msg),
        third_sig_msg: Some(*third_sig_msg),
        signer_pub_key_packed: signer_pub_key_packed.to_vec(),
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.after.clone(),
        rhs: deposit_witness.after.clone(),
        signature_data: signature_data.clone(),
    };

    let operation_three = Operation {
        new_root: deposit_witness.after_root,
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("3").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[3]),
        first_sig_msg: Some(*first_sig_msg),
        second_sig_msg: Some(*second_sig_msg),
        third_sig_msg: Some(*third_sig_msg),
        signer_pub_key_packed: signer_pub_key_packed.to_vec(),
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.after.clone(),
        rhs: deposit_witness.after.clone(),
        signature_data: signature_data.clone(),
    };

    let operation_four = Operation {
        new_root: deposit_witness.after_root,
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("4").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[4]),
        first_sig_msg: Some(*first_sig_msg),
        second_sig_msg: Some(*second_sig_msg),
        third_sig_msg: Some(*third_sig_msg),
        signer_pub_key_packed: signer_pub_key_packed.to_vec(),
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.after.clone(),
        rhs: deposit_witness.after.clone(),
        signature_data: signature_data.clone(),
    };

    let operation_five = Operation {
        new_root: deposit_witness.after_root,
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("5").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[5]),
        first_sig_msg: Some(*first_sig_msg),
        second_sig_msg: Some(*second_sig_msg),
        third_sig_msg: Some(*third_sig_msg),
        signer_pub_key_packed: signer_pub_key_packed.to_vec(),
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.after.clone(),
        rhs: deposit_witness.after.clone(),
        signature_data: signature_data.clone(),
    };
    let operations: Vec<Operation<_>> = vec![
        operation_zero,
        operation_one,
        operation_two,
        operation_three,
        operation_four,
        operation_five,
    ];
    operations
}
#[cfg(test)]
mod test {
    use super::*;
    use crate::witness::test_utils::{check_circuit, test_genesis_plasma_state};
    use bigdecimal::BigDecimal;
    use franklin_crypto::bellman::pairing::ff::Field;
    use models::node::{Account, AccountAddress, Address, Deposit};

    #[test]
    #[ignore]
    fn test_deposit_franklin_in_empty_leaf() {
        let (mut plasma_state, mut witness_accum) = test_genesis_plasma_state(Vec::new());

        let empty_account_id = 1;
        let deposit_op = DepositOp {
            priority_op: Deposit {
                sender: Address::zero(),
                token: 0,
                amount: BigDecimal::from(1),
                account: AccountAddress::from_hex("sync:1111111111111111111111111111111111111111")
                    .unwrap(),
            },
            account_id: empty_account_id,
        };

        plasma_state.apply_deposit_op(&deposit_op);

        let deposit_witness = apply_deposit_tx(&mut witness_accum.account_tree, &deposit_op);
        let deposit_operations = calculate_deposit_operations_from_witness(
            &deposit_witness,
            &Fr::zero(),
            &Fr::zero(),
            &Fr::zero(),
            &SignatureData {
                r_packed: vec![Some(false); 256],
                s: vec![Some(false); 256],
            },
            &[Some(false); 256], //doesn't matter for deposit
        );
        let pub_data_from_witness = deposit_witness.get_pubdata();

        witness_accum.add_operation_with_pubdata(deposit_operations, pub_data_from_witness);
        witness_accum.collect_fees(&Vec::new());
        witness_accum.calculate_pubdata_commitment();

        assert_eq!(
            plasma_state.root_hash(),
            witness_accum
                .root_after_fees
                .expect("witness accum after root hash empty"),
            "root hash in state keeper and witness generation code mismatch"
        );

        check_circuit(witness_accum.into_circuit_instance());
    }

    #[test]
    #[ignore]
    fn test_deposit_franklin_existing_account() {
        let deposit_to_account_id = 1;
        let deposit_to_account_address =
            AccountAddress::from_hex("sync:1111111111111111111111111111111111111111").unwrap();
        let (mut plasma_state, mut witness_accum) = test_genesis_plasma_state(vec![(
            deposit_to_account_id,
            Account::default_with_address(&deposit_to_account_address),
        )]);

        let deposit_op = DepositOp {
            priority_op: Deposit {
                sender: Address::zero(),
                token: 0,
                amount: BigDecimal::from(1),
                account: deposit_to_account_address,
            },
            account_id: deposit_to_account_id,
        };

        plasma_state.apply_deposit_op(&deposit_op);

        let deposit_witness = apply_deposit_tx(&mut witness_accum.account_tree, &deposit_op);
        let deposit_operations = calculate_deposit_operations_from_witness(
            &deposit_witness,
            &Fr::zero(),
            &Fr::zero(),
            &Fr::zero(),
            &SignatureData {
                r_packed: vec![Some(false); 256],
                s: vec![Some(false); 256],
            },
            &[Some(false); 256], //doesn't matter for deposit
        );
        let pub_data_from_witness = deposit_witness.get_pubdata();

        witness_accum.add_operation_with_pubdata(deposit_operations, pub_data_from_witness);
        witness_accum.collect_fees(&Vec::new());
        witness_accum.calculate_pubdata_commitment();

        assert_eq!(
            plasma_state.root_hash(),
            witness_accum
                .root_after_fees
                .expect("witness accum after root hash empty"),
            "root hash in state keeper and witness generation code mismatch"
        );

        check_circuit(witness_accum.into_circuit_instance());
    }

    #[test]
    // #[ignore]
    fn test_deposit_franklin_existing_account_with_transpiler() {
        let deposit_to_account_id = 1;
        let deposit_to_account_address =
            AccountAddress::from_hex("sync:1111111111111111111111111111111111111111").unwrap();
        let (mut plasma_state, mut witness_accum) = test_genesis_plasma_state(vec![(
            deposit_to_account_id,
            Account::default_with_address(&deposit_to_account_address),
        )]);

        let deposit_op = DepositOp {
            priority_op: Deposit {
                sender: Address::zero(),
                token: 0,
                amount: BigDecimal::from(1),
                account: deposit_to_account_address,
            },
            account_id: deposit_to_account_id,
        };

        plasma_state.apply_deposit_op(&deposit_op);

        let deposit_witness = apply_deposit_tx(&mut witness_accum.account_tree, &deposit_op);
        let deposit_operations = calculate_deposit_operations_from_witness(
            &deposit_witness,
            &Fr::zero(),
            &Fr::zero(),
            &Fr::zero(),
            &SignatureData {
                r_packed: vec![Some(false); 256],
                s: vec![Some(false); 256],
            },
            &[Some(false); 256], //doesn't matter for deposit
        );
        let pub_data_from_witness = deposit_witness.get_pubdata();

        witness_accum.add_operation_with_pubdata(deposit_operations, pub_data_from_witness);
        witness_accum.collect_fees(&Vec::new());
        witness_accum.calculate_pubdata_commitment();

        assert_eq!(
            plasma_state.root_hash(),
            witness_accum
                .root_after_fees
                .expect("witness accum after root hash empty"),
            "root hash in state keeper and witness generation code mismatch"
        );

        use franklin_crypto::bellman::plonk::adaptor::alternative::*;
        use franklin_crypto::bellman::plonk::plonk::generator::*;
        use franklin_crypto::bellman::plonk::plonk::prover::*;
        use franklin_crypto::bellman::pairing::bn256::Bn256;

        use franklin_crypto::bellman::Circuit;

        let mut transpiler = Transpiler::new();

        let c = witness_accum.into_circuit_instance();

        c.clone().synthesize(&mut transpiler).unwrap();
    
        let hints = transpiler.into_hints();

        use franklin_crypto::bellman::plonk::cs::Circuit as PlonkCircuit;

        let adapted_curcuit = AdaptorCircuit::new(c.clone(), &hints);

        let mut assembly = GeneratorAssembly::<Bn256>::new();
        adapted_curcuit.synthesize(&mut assembly).unwrap();
        assembly.finalize();

        println!("Transpiled into {} gates", assembly.num_gates());

        println!("Trying to prove");

        let adapted_curcuit = AdaptorCircuit::new(c.clone(), &hints);

        let mut prover = ProvingAssembly::<Bn256>::new();
        adapted_curcuit.synthesize(&mut prover).unwrap();
        prover.finalize();

        // assert!(prover.is_satisfied());
    }
}