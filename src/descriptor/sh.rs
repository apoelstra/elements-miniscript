// Miniscript
// Written in 2020 by rust-miniscript developers
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

//! # P2SH Descriptors
//!
//! Implementation of p2sh descriptors. Contains the implementation
//! of sh, wrapped fragments for sh which include wsh, sortedmulti
//! sh(miniscript), and sh(wpkh)
//!

use std::{fmt, str::FromStr};

use bitcoin::{self, blockdata::script, Script};

use expression::{self, FromTree};
use miniscript::context::ScriptContext;
use policy::{semantic, Liftable};
use push_opcode_size;
use util::{varint_len, witness_to_scriptsig};
use {Error, Legacy, Miniscript, MiniscriptKey, Satisfier, Segwitv0, ToPublicKey};

use super::{
    checksum::{desc_checksum, verify_checksum},
    DescriptorTrait, PkTranslate, SortedMultiVec, Wpkh, Wsh,
};

/// A Legacy p2sh Descriptor
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct Sh<Pk: MiniscriptKey> {
    /// underlying miniscript
    inner: ShInner<Pk>,
}

/// Sh Inner
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq)]
enum ShInner<Pk: MiniscriptKey> {
    /// Nested Wsh
    Wsh(Wsh<Pk>),
    /// Nested Wpkh
    Wpkh(Wpkh<Pk>),
    /// Inner Sorted Multi
    SortedMulti(SortedMultiVec<Pk, Legacy>),
    /// p2sh miniscript
    Ms(Miniscript<Pk, Legacy>),
}

impl<Pk: MiniscriptKey> Liftable<Pk> for Sh<Pk> {
    fn lift(&self) -> Result<semantic::Policy<Pk>, Error> {
        match self.inner {
            ShInner::Wsh(ref wsh) => wsh.lift(),
            ShInner::Wpkh(ref pk) => Ok(semantic::Policy::KeyHash(pk.as_inner().to_pubkeyhash())),
            ShInner::SortedMulti(ref smv) => smv.lift(),
            ShInner::Ms(ref ms) => ms.lift(),
        }
    }
}

impl<Pk: MiniscriptKey> fmt::Debug for Sh<Pk> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.inner {
            ShInner::Wsh(ref wsh_inner) => write!(f, "sh({:?})", wsh_inner),
            ShInner::Wpkh(ref pk) => write!(f, "sh({:?})", pk),
            ShInner::SortedMulti(ref smv) => write!(f, "sh({:?})", smv),
            ShInner::Ms(ref ms) => write!(f, "sh({:?})", ms),
        }
    }
}

impl<Pk: MiniscriptKey> fmt::Display for Sh<Pk> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let desc = match self.inner {
            // extra nesting because the impl of "{}" returns the checksum
            // which we don't want
            ShInner::Wsh(ref wsh) => match wsh.as_inner() {
                super::segwitv0::WshInner::SortedMulti(ref smv) => format!("sh(wsh({}))", smv),
                super::segwitv0::WshInner::Ms(ref ms) => format!("sh(wsh({}))", ms),
            },
            ShInner::Wpkh(ref pk) => format!("sh({})", pk),
            ShInner::SortedMulti(ref smv) => format!("sh({})", smv),
            ShInner::Ms(ref ms) => format!("sh({})", ms),
        };
        let checksum = desc_checksum(&desc).map_err(|_| fmt::Error)?;
        write!(f, "{}#{}", &desc, &checksum)
    }
}

impl<Pk: MiniscriptKey> FromTree for Sh<Pk>
where
    <Pk as FromStr>::Err: ToString,
    <<Pk as MiniscriptKey>::Hash as FromStr>::Err: ToString,
{
    fn from_tree(top: &expression::Tree) -> Result<Self, Error> {
        if top.name == "sh" && top.args.len() == 1 {
            let top = &top.args[0];
            let inner = match top.name {
                "wsh" => ShInner::Wsh(Wsh::from_tree(&top)?),
                "wpkh" => ShInner::Wpkh(Wpkh::from_tree(&top)?),
                "sortedmulti" => ShInner::SortedMulti(SortedMultiVec::from_tree(&top)?),
                _ => {
                    let sub = Miniscript::from_tree(&top)?;
                    Legacy::top_level_checks(&sub)?;
                    ShInner::Ms(sub)
                }
            };
            Ok(Sh { inner: inner })
        } else {
            Err(Error::Unexpected(format!(
                "{}({} args) while parsing sh descriptor",
                top.name,
                top.args.len(),
            )))
        }
    }
}

impl<Pk: MiniscriptKey> FromStr for Sh<Pk>
where
    <Pk as FromStr>::Err: ToString,
    <<Pk as MiniscriptKey>::Hash as FromStr>::Err: ToString,
{
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let desc_str = verify_checksum(s)?;
        let top = expression::Tree::from_str(desc_str)?;
        Self::from_tree(&top)
    }
}

impl<Pk: MiniscriptKey> Sh<Pk> {
    /// Create a new p2sh descriptor with the raw miniscript
    pub fn new(ms: Miniscript<Pk, Legacy>) -> Result<Self, Error> {
        // do the top-level checks
        Legacy::top_level_checks(&ms)?;
        Ok(Self {
            inner: ShInner::Ms(ms),
        })
    }

    /// Create a new p2sh sortedmulti descriptor with threshold `k`
    /// and Vec of `pks`.
    pub fn new_sortedmulti(k: usize, pks: Vec<Pk>) -> Result<Self, Error> {
        // The context checks will be carried out inside new function for
        // sortedMultiVec
        Ok(Self {
            inner: ShInner::SortedMulti(SortedMultiVec::new(k, pks)?),
        })
    }

    /// Create a new p2sh wrapped wsh descriptor with the raw miniscript
    pub fn new_wsh(ms: Miniscript<Pk, Segwitv0>) -> Result<Self, Error> {
        Ok(Self {
            inner: ShInner::Wsh(Wsh::new(ms)?),
        })
    }

    /// Create a new p2sh wrapped wsh sortedmulti descriptor from threshold
    /// `k` and Vec of `pks`
    pub fn new_wsh_sortedmulti(k: usize, pks: Vec<Pk>) -> Result<Self, Error> {
        // The context checks will be carried out inside new function for
        // sortedMultiVec
        Ok(Self {
            inner: ShInner::Wsh(Wsh::new_sortedmulti(k, pks)?),
        })
    }

    /// Create a new p2sh wrapped wpkh from `Pk`
    pub fn new_wpkh(pk: Pk) -> Result<Self, Error> {
        Ok(Self {
            inner: ShInner::Wpkh(Wpkh::new(pk)?),
        })
    }
}

impl<Pk: MiniscriptKey> DescriptorTrait<Pk> for Sh<Pk>
where
    <Pk as FromStr>::Err: ToString,
    <<Pk as MiniscriptKey>::Hash as FromStr>::Err: ToString,
{
    fn sanity_check(&self) -> Result<(), Error> {
        match self.inner {
            ShInner::Wsh(ref wsh) => wsh.sanity_check()?,
            ShInner::Wpkh(ref wpkh) => wpkh.sanity_check()?,
            ShInner::SortedMulti(ref smv) => smv.sanity_check()?,
            ShInner::Ms(ref ms) => ms.sanity_check()?,
        }
        Ok(())
    }

    fn address<ToPkCtx: Copy>(
        &self,
        to_pk_ctx: ToPkCtx,
        network: bitcoin::Network,
    ) -> Option<bitcoin::Address>
    where
        Pk: ToPublicKey<ToPkCtx>,
    {
        match self.inner {
            ShInner::Wsh(ref wsh) => Some(bitcoin::Address::p2sh(
                &wsh.script_pubkey(to_pk_ctx),
                network,
            )),
            ShInner::Wpkh(ref wpkh) => Some(bitcoin::Address::p2sh(
                &wpkh.script_pubkey(to_pk_ctx),
                network,
            )),
            ShInner::SortedMulti(ref smv) => {
                Some(bitcoin::Address::p2sh(&smv.encode(to_pk_ctx), network))
            }
            ShInner::Ms(ref ms) => Some(bitcoin::Address::p2sh(&ms.encode(to_pk_ctx), network)),
        }
    }

    fn script_pubkey<ToPkCtx: Copy>(&self, to_pk_ctx: ToPkCtx) -> Script
    where
        Pk: ToPublicKey<ToPkCtx>,
    {
        match self.inner {
            ShInner::Wsh(ref wsh) => wsh.script_pubkey(to_pk_ctx).to_p2sh(),
            ShInner::Wpkh(ref wpkh) => wpkh.script_pubkey(to_pk_ctx).to_p2sh(),
            ShInner::SortedMulti(ref smv) => smv.encode(to_pk_ctx).to_p2sh(),
            ShInner::Ms(ref ms) => ms.encode(to_pk_ctx).to_p2sh(),
        }
    }

    fn unsigned_script_sig<ToPkCtx: Copy>(&self, to_pk_ctx: ToPkCtx) -> Script
    where
        Pk: ToPublicKey<ToPkCtx>,
    {
        match self.inner {
            ShInner::Wsh(ref wsh) => {
                let witness_script = wsh.witness_script(to_pk_ctx);
                script::Builder::new()
                    .push_slice(&witness_script.to_v0_p2wsh()[..])
                    .into_script()
            }
            ShInner::Wpkh(ref wpkh) => {
                let redeem_script = wpkh.script_pubkey(to_pk_ctx);
                script::Builder::new()
                    .push_slice(&redeem_script[..])
                    .into_script()
            }
            ShInner::SortedMulti(..) | ShInner::Ms(..) => Script::new(),
        }
    }

    fn witness_script<ToPkCtx: Copy>(&self, to_pk_ctx: ToPkCtx) -> Script
    where
        Pk: ToPublicKey<ToPkCtx>,
    {
        match self.inner {
            ShInner::Wsh(ref wsh) => wsh.witness_script(to_pk_ctx),
            ShInner::Wpkh(ref wpkh) => wpkh.script_pubkey(to_pk_ctx),
            ShInner::SortedMulti(ref smv) => smv.encode(to_pk_ctx),
            ShInner::Ms(ref ms) => ms.encode(to_pk_ctx),
        }
    }

    fn get_satisfaction<ToPkCtx, S>(
        &self,
        satisfier: S,
        to_pk_ctx: ToPkCtx,
    ) -> Result<(Vec<Vec<u8>>, Script), Error>
    where
        ToPkCtx: Copy,
        Pk: ToPublicKey<ToPkCtx>,
        S: Satisfier<ToPkCtx, Pk>,
    {
        let script_sig = self.unsigned_script_sig(to_pk_ctx);
        match self.inner {
            ShInner::Wsh(ref wsh) => {
                let (witness, _) = wsh.get_satisfaction(satisfier, to_pk_ctx)?;
                Ok((witness, script_sig))
            }
            ShInner::Wpkh(ref wpkh) => {
                let (witness, _) = wpkh.get_satisfaction(satisfier, to_pk_ctx)?;
                Ok((witness, script_sig))
            }
            ShInner::SortedMulti(ref smv) => {
                let mut script_witness = smv.satisfy(satisfier, to_pk_ctx)?;
                script_witness.push(smv.encode(to_pk_ctx).into_bytes());
                let script_sig = witness_to_scriptsig(&script_witness);
                let witness = vec![];
                Ok((witness, script_sig))
            }
            ShInner::Ms(ref ms) => {
                let mut script_witness = ms.satisfy(satisfier, to_pk_ctx)?;
                script_witness.push(ms.encode(to_pk_ctx).into_bytes());
                let script_sig = witness_to_scriptsig(&script_witness);
                let witness = vec![];
                Ok((witness, script_sig))
            }
        }
    }

    fn max_satisfaction_weight(&self) -> Option<usize> {
        Some(match self.inner {
            // add weighted script sig, len byte stays the same
            ShInner::Wsh(ref wsh) => 4 * 35 + wsh.max_satisfaction_weight()?,
            ShInner::SortedMulti(ref smv) => {
                let ss = smv.script_size();
                let ps = push_opcode_size(ss);
                let scriptsig_len = ps + ss + smv.max_satisfaction_size();
                4 * (varint_len(scriptsig_len) + scriptsig_len)
            }
            // add weighted script sig, len byte stays the same
            ShInner::Wpkh(ref wpkh) => 4 * 23 + wpkh.max_satisfaction_weight()?,
            ShInner::Ms(ref ms) => {
                let ss = ms.script_size();
                let ps = push_opcode_size(ss);
                let scriptsig_len = ps + ss + ms.max_satisfaction_size()?;
                4 * (varint_len(scriptsig_len) + scriptsig_len)
            }
        })
    }

    fn script_code<ToPkCtx: Copy>(&self, to_pk_ctx: ToPkCtx) -> Script
    where
        Pk: ToPublicKey<ToPkCtx>,
    {
        match self.inner {
            //     - For P2WSH witness program, if the witnessScript does not contain any `OP_CODESEPARATOR`,
            //       the `scriptCode` is the `witnessScript` serialized as scripts inside CTxOut.
            ShInner::Wsh(ref wsh) => wsh.script_code(to_pk_ctx),
            ShInner::SortedMulti(ref smv) => smv.encode(to_pk_ctx),
            ShInner::Wpkh(ref wpkh) => wpkh.script_code(to_pk_ctx),
            // For "legacy" P2SH outputs, it is defined as the txo's redeemScript.
            ShInner::Ms(ref ms) => ms.encode(to_pk_ctx),
        }
    }
}

impl<P: MiniscriptKey, Q: MiniscriptKey> PkTranslate<P, Q> for Sh<P> {
    type Output = Sh<Q>;

    fn translate_pk<Fpk, Fpkh, E>(
        &self,
        mut translatefpk: Fpk,
        mut translatefpkh: Fpkh,
    ) -> Result<Self::Output, E>
    where
        Fpk: FnMut(&P) -> Result<Q, E>,
        Fpkh: FnMut(&P::Hash) -> Result<Q::Hash, E>,
        Q: MiniscriptKey,
    {
        let inner = match self.inner {
            ShInner::Wsh(ref wsh) => {
                ShInner::Wsh(wsh.translate_pk(&mut translatefpk, &mut translatefpkh)?)
            }
            ShInner::Wpkh(ref wpkh) => {
                ShInner::Wpkh(wpkh.translate_pk(&mut translatefpk, &mut translatefpkh)?)
            }
            ShInner::SortedMulti(ref smv) => {
                ShInner::SortedMulti(smv.translate_pk(&mut translatefpk)?)
            }
            ShInner::Ms(ref ms) => {
                ShInner::Ms(ms.translate_pk(&mut translatefpk, &mut translatefpkh)?)
            }
        };
        Ok(Sh { inner: inner })
    }
}
