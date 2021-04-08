// Miniscript
// Written in 2020 by
//     Sanket Kanjular and Andrew Poelstra
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! Interpreter stack

use std::ops::Index;

use bitcoin;
use elements::hashes::{hash160, ripemd160, sha256, sha256d, Hash};
use elements::{self, opcodes, script};

use {ElementsSig, ToPublicKey};

use super::{verify_sersig, Error, HashLockType, SatisfiedConstraint};
use miniscript::limits::{MAX_SCRIPT_ELEMENT_SIZE, MAX_STANDARD_P2WSH_STACK_ITEM_SIZE};
use util;
/// Definition of Stack Element of the Stack used for interpretation of Miniscript.
/// All stack elements with vec![] go to Dissatisfied and vec![1] are marked to Satisfied.
/// Others are directly pushed as witness
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Element<'txin> {
    /// Result of a satisfied Miniscript fragment
    /// Translated from `vec![1]` from input stack
    Satisfied,
    /// Result of a dissatisfied Miniscript fragment
    /// Translated from `vec![]` from input stack
    Dissatisfied,
    /// Input from the witness stack
    Push(&'txin [u8]),
}

impl<'txin> From<&'txin Vec<u8>> for Element<'txin> {
    fn from(v: &'txin Vec<u8>) -> Element<'txin> {
        From::from(&v[..])
    }
}

impl<'txin> From<&'txin [u8]> for Element<'txin> {
    fn from(v: &'txin [u8]) -> Element<'txin> {
        if *v == [1] {
            Element::Satisfied
        } else if v.is_empty() {
            Element::Dissatisfied
        } else {
            Element::Push(v)
        }
    }
}

impl<'txin> Element<'txin> {
    /// Converts a Bitcoin `script::Instruction` to a stack element
    ///
    /// Supports `OP_1` but no other numbers since these are not used by Miniscript
    pub fn from_instruction(
        ins: Result<script::Instruction<'txin>, elements::script::Error>,
    ) -> Result<Self, Error> {
        match ins {
            //Also covers the dissatisfied case as PushBytes0
            Ok(script::Instruction::PushBytes(v)) => Ok(Element::from(v)),
            Ok(script::Instruction::Op(opcodes::all::OP_PUSHNUM_1)) => Ok(Element::Satisfied),
            _ => Err(Error::ExpectedPush),
        }
    }

    /// Panics when the element is not a push
    pub(crate) fn as_push(&self) -> &[u8] {
        match self {
            Element::Push(x) => x,
            _ => unreachable!("Called as_push on 1/0 stack elem"),
        }
    }

    /// Errs when the element is not a push
    pub(crate) fn try_push(&self) -> Result<&[u8], Error> {
        match self {
            Element::Push(x) => Ok(x),
            _ => Err(Error::ExpectedPush),
        }
    }

    /// Convert element into slice
    pub(crate) fn into_slice(self) -> &'txin [u8] {
        match self {
            Element::Satisfied => &[1],
            Element::Dissatisfied => &[],
            Element::Push(ref v) => v,
        }
    }
}

/// Stack Data structure representing the stack input to Miniscript. This Stack
/// is created from the combination of ScriptSig and Witness stack.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Stack<'txin>(pub(super) Vec<Element<'txin>>);

impl<'txin> From<Vec<Element<'txin>>> for Stack<'txin> {
    fn from(v: Vec<Element<'txin>>) -> Self {
        Stack(v)
    }
}

impl<'txin> Default for Stack<'txin> {
    fn default() -> Self {
        Stack(vec![])
    }
}

impl<'txin> Index<usize> for Stack<'txin> {
    type Output = Element<'txin>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<'txin> Stack<'txin> {
    /// Whether the stack is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of elements on the stack
    pub fn len(&mut self) -> usize {
        self.0.len()
    }

    /// Removes the top stack element, if the stack is nonempty
    pub fn pop(&mut self) -> Option<Element<'txin>> {
        self.0.pop()
    }

    /// Pushes an element onto the top of the stack
    pub fn push(&mut self, elem: Element<'txin>) -> () {
        self.0.push(elem);
    }

    /// Returns a new stack representing the top `k` elements of the stack,
    /// removing these elements from the original
    pub fn split_off(&mut self, k: usize) -> Vec<Element<'txin>> {
        self.0.split_off(k)
    }

    /// Returns a reference to the top stack element, if the stack is nonempty
    pub fn last(&self) -> Option<&Element<'txin>> {
        self.0.last()
    }

    /// Helper function to evaluate a Pk Node which takes the
    /// top of the stack as input signature and validates it.
    /// Sat: If the signature witness is correct, 1 is pushed
    /// Unsat: For empty witness a 0 is pushed
    /// Err: All of other witness result in errors.
    /// `pk` CHECKSIG
    pub fn evaluate_pk<'intp, F>(
        &mut self,
        verify_sig: F,
        pk: &'intp bitcoin::PublicKey,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>>
    where
        F: FnMut(&bitcoin::PublicKey, ElementsSig) -> bool,
    {
        if let Some(sigser) = self.pop() {
            match sigser {
                Element::Dissatisfied => {
                    self.push(Element::Dissatisfied);
                    None
                }
                Element::Push(ref sigser) => {
                    let sig = verify_sersig(verify_sig, pk, sigser);
                    match sig {
                        Ok(sig) => {
                            self.push(Element::Satisfied);
                            Some(Ok(SatisfiedConstraint::PublicKey { key: pk, sig }))
                        }
                        Err(e) => return Some(Err(e)),
                    }
                }
                Element::Satisfied => {
                    return Some(Err(Error::PkEvaluationError(pk.clone().to_public_key())))
                }
            }
        } else {
            Some(Err(Error::UnexpectedStackEnd))
        }
    }

    /// Helper function to evaluate a Pkh Node. Takes input as pubkey and sig
    /// from the top of the stack and outputs Sat if the pubkey, sig is valid
    /// Sat: If the pubkey hash matches and signature witness is correct,
    /// Unsat: For an empty witness
    /// Err: All of other witness result in errors.
    /// `DUP HASH160 <keyhash> EQUALVERIY CHECKSIG`
    pub fn evaluate_pkh<'intp, F>(
        &mut self,
        verify_sig: F,
        pkh: &'intp hash160::Hash,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>>
    where
        F: FnOnce(&bitcoin::PublicKey, ElementsSig) -> bool,
    {
        if let Some(Element::Push(pk)) = self.pop() {
            let pk_hash = hash160::Hash::hash(pk);
            if pk_hash != *pkh {
                return Some(Err(Error::PkHashVerifyFail(*pkh)));
            }
            match bitcoin::PublicKey::from_slice(pk) {
                Ok(pk) => {
                    if let Some(sigser) = self.pop() {
                        match sigser {
                            Element::Dissatisfied => {
                                self.push(Element::Dissatisfied);
                                None
                            }
                            Element::Push(sigser) => {
                                let sig = verify_sersig(verify_sig, &pk, sigser);
                                match sig {
                                    Ok(sig) => {
                                        self.push(Element::Satisfied);
                                        Some(Ok(SatisfiedConstraint::PublicKeyHash {
                                            keyhash: pkh,
                                            key: pk,
                                            sig,
                                        }))
                                    }
                                    Err(e) => return Some(Err(e)),
                                }
                            }
                            Element::Satisfied => {
                                return Some(Err(Error::PkEvaluationError(
                                    pk.clone().to_public_key(),
                                )))
                            }
                        }
                    } else {
                        Some(Err(Error::UnexpectedStackEnd))
                    }
                }
                Err(..) => Some(Err(Error::PubkeyParseError)),
            }
        } else {
            Some(Err(Error::UnexpectedStackEnd))
        }
    }

    /// Helper function to evaluate a After Node. Takes no argument from stack
    /// `n CHECKLOCKTIMEVERIFY 0NOTEQUAL` and `n CHECKLOCKTIMEVERIFY`
    /// Ideally this should return int value as n: build_scriptint(t as i64)),
    /// The reason we don't need to copy the Script semantics is that
    /// Miniscript never evaluates integers and it is safe to treat them as
    /// booleans
    pub fn evaluate_after<'intp>(
        &mut self,
        n: &'intp u32,
        age: u32,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>> {
        if age >= *n {
            self.push(Element::Satisfied);
            Some(Ok(SatisfiedConstraint::AbsoluteTimeLock { time: n }))
        } else {
            Some(Err(Error::AbsoluteLocktimeNotMet(*n)))
        }
    }

    /// Helper function to evaluate a Older Node. Takes no argument from stack
    /// `n CHECKSEQUENCEVERIFY 0NOTEQUAL` and `n CHECKSEQUENCEVERIFY`
    /// Ideally this should return int value as n: build_scriptint(t as i64)),
    /// The reason we don't need to copy the Script semantics is that
    /// Miniscript never evaluates integers and it is safe to treat them as
    /// booleans
    pub fn evaluate_older<'intp>(
        &mut self,
        n: &'intp u32,
        height: u32,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>> {
        if height >= *n {
            self.push(Element::Satisfied);
            Some(Ok(SatisfiedConstraint::RelativeTimeLock { time: n }))
        } else {
            Some(Err(Error::RelativeLocktimeNotMet(*n)))
        }
    }

    /// Helper function to evaluate a Sha256 Node.
    /// `SIZE 32 EQUALVERIFY SHA256 h EQUAL`
    pub fn evaluate_sha256<'intp>(
        &mut self,
        hash: &'intp sha256::Hash,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>> {
        if let Some(Element::Push(preimage)) = self.pop() {
            if preimage.len() != 32 {
                return Some(Err(Error::HashPreimageLengthMismatch));
            }
            if sha256::Hash::hash(preimage) == *hash {
                self.push(Element::Satisfied);
                Some(Ok(SatisfiedConstraint::HashLock {
                    hash: HashLockType::Sha256(hash),
                    preimage,
                }))
            } else {
                self.push(Element::Dissatisfied);
                None
            }
        } else {
            Some(Err(Error::UnexpectedStackEnd))
        }
    }

    /// Helper function to evaluate a Hash256 Node.
    /// `SIZE 32 EQUALVERIFY HASH256 h EQUAL`
    pub fn evaluate_hash256<'intp>(
        &mut self,
        hash: &'intp sha256d::Hash,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>> {
        if let Some(Element::Push(preimage)) = self.pop() {
            if preimage.len() != 32 {
                return Some(Err(Error::HashPreimageLengthMismatch));
            }
            if sha256d::Hash::hash(preimage) == *hash {
                self.push(Element::Satisfied);
                Some(Ok(SatisfiedConstraint::HashLock {
                    hash: HashLockType::Hash256(hash),
                    preimage,
                }))
            } else {
                self.push(Element::Dissatisfied);
                None
            }
        } else {
            Some(Err(Error::UnexpectedStackEnd))
        }
    }

    /// Helper function to evaluate a Hash160 Node.
    /// `SIZE 32 EQUALVERIFY HASH160 h EQUAL`
    pub fn evaluate_hash160<'intp>(
        &mut self,
        hash: &'intp hash160::Hash,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>> {
        if let Some(Element::Push(preimage)) = self.pop() {
            if preimage.len() != 32 {
                return Some(Err(Error::HashPreimageLengthMismatch));
            }
            if hash160::Hash::hash(preimage) == *hash {
                self.push(Element::Satisfied);
                Some(Ok(SatisfiedConstraint::HashLock {
                    hash: HashLockType::Hash160(hash),
                    preimage,
                }))
            } else {
                self.push(Element::Dissatisfied);
                None
            }
        } else {
            Some(Err(Error::UnexpectedStackEnd))
        }
    }

    /// Helper function to evaluate a RipeMd160 Node.
    /// `SIZE 32 EQUALVERIFY RIPEMD160 h EQUAL`
    pub fn evaluate_ripemd160<'intp>(
        &mut self,
        hash: &'intp ripemd160::Hash,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>> {
        if let Some(Element::Push(preimage)) = self.pop() {
            if preimage.len() != 32 {
                return Some(Err(Error::HashPreimageLengthMismatch));
            }
            if ripemd160::Hash::hash(preimage) == *hash {
                self.push(Element::Satisfied);
                Some(Ok(SatisfiedConstraint::HashLock {
                    hash: HashLockType::Ripemd160(hash),
                    preimage,
                }))
            } else {
                self.push(Element::Dissatisfied);
                None
            }
        } else {
            Some(Err(Error::UnexpectedStackEnd))
        }
    }

    /// Evaluate a ver fragment. Get the version from the global stack
    /// context and check equality
    pub fn evaluate_ver<'intp>(
        &mut self,
        n: &'intp u32,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>> {
        // Version is at index 11
        let ver = self[11];
        if let Err(e) = ver.try_push() {
            return Some(Err(e));
        }
        let elem = ver.as_push();
        if elem.len() == 4 {
            let wit_ver = util::slice_to_u32_le(elem);
            if wit_ver == *n {
                self.push(Element::Satisfied);
                Some(Ok(SatisfiedConstraint::VerEq { n: n }))
            } else {
                None
            }
        } else {
            Some(Err(Error::CovWitnessSizeErr {
                pos: 1,
                expected: 4,
                actual: elem.len(),
            }))
        }
    }

    /// Evaluate a output_pref fragment. Get the hashoutputs from the global
    /// stack context and check it's preimage starts with prefix.
    /// The user provides the suffix as witness in 6 different elements
    pub fn evaluate_outputs_pref<'intp>(
        &mut self,
        pref: &'intp [u8],
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>> {
        // Version is at index 1
        let hash_outputs = self[3];
        if let Err(e) = hash_outputs.try_push() {
            return Some(Err(e));
        }
        // Maximum number of suffix elements
        let max_elems = MAX_SCRIPT_ELEMENT_SIZE / MAX_STANDARD_P2WSH_STACK_ITEM_SIZE + 1;
        let hash_outputs = hash_outputs.as_push();
        if hash_outputs.len() == 32 {
            // We want to cat the last 6 elements(5 cats) in suffix
            if self.len() < max_elems {
                return Some(Err(Error::UnexpectedStackEnd));
            }
            let mut outputs_builder = Vec::new();
            outputs_builder.extend(pref);
            let len = self.len();
            // Add the max_elems suffix elements
            for i in 0..max_elems {
                outputs_builder.extend(self[len - max_elems + i].into_slice());
            }
            // Pop the max_elems suffix elements
            for _ in 0..max_elems {
                self.pop().unwrap();
            }
            if sha256d::Hash::hash(&outputs_builder).as_inner() == hash_outputs {
                self.push(Element::Satisfied);
                Some(Ok(SatisfiedConstraint::OutputsPref { pref: pref }))
            } else {
                None
            }
        } else {
            Some(Err(Error::CovWitnessSizeErr {
                pos: 9,
                expected: 32,
                actual: hash_outputs.len(),
            }))
        }
    }

    /// Helper function to evaluate a checkmultisig which takes the top of the
    /// stack as input signatures and validates it in order of pubkeys.
    /// For example, if the first signature is satisfied by second public key,
    /// other signatures are not checked against the first pubkey.
    /// `multi(2,pk1,pk2)` would be satisfied by `[0 sig2 sig1]` and Err on
    /// `[0 sig2 sig1]`
    pub fn evaluate_multi<'intp, F>(
        &mut self,
        verify_sig: F,
        pk: &'intp bitcoin::PublicKey,
    ) -> Option<Result<SatisfiedConstraint<'intp, 'txin>, Error>>
    where
        F: FnOnce(&bitcoin::PublicKey, ElementsSig) -> bool,
    {
        if let Some(witness_sig) = self.pop() {
            if let Element::Push(sigser) = witness_sig {
                let sig = verify_sersig(verify_sig, pk, sigser);
                match sig {
                    Ok(sig) => return Some(Ok(SatisfiedConstraint::PublicKey { key: pk, sig })),
                    Err(..) => {
                        self.push(witness_sig);
                        return None;
                    }
                }
            } else {
                Some(Err(Error::UnexpectedStackBoolean))
            }
        } else {
            Some(Err(Error::UnexpectedStackEnd))
        }
    }
}
