// Simplicity "Human-Readable" Language
// Written in 2023 by
//   Andrew Poelstra <simplicity@wpsoftware.net>
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

mod call_jet;

use std::collections::HashMap;
use std::sync::Arc;

use elements_miniscript::bitcoin::secp256k1::{Keypair, Message, Secp256k1, SecretKey};
use elements_miniscript::bitcoin::XOnlyPublicKey;
use elements_miniscript::Descriptor;
use elements_miniscript::descriptor::TapTree;
use elements_miniscript::elements::{self, AddressParams};
use elements_miniscript::elements::taproot::{LeafVersion, TapLeafHash};
use elements_miniscript::elements::schnorr::SchnorrSig;
use elements_miniscript::elements::sighash::SchnorrSighashType;
use elements_miniscript::bitcoin::hashes::Hash as _;
use elements_miniscript::bitcoin::hex::{FromHex as _, DisplayHex as _};

use simplicity::jet::Elements; 
use simplicity::jet::elements::{ElementsEnv, ElementsUtxo}; 

fn main() -> Result<(), String> {
    // From BIP 341
    let unspendable_key = XOnlyPublicKey::from_slice(&[
        0x50, 0x92, 0x9b, 0x74, 0xc1, 0xa0, 0x49, 0x54,
        0xb7, 0x8b, 0x4b, 0x60, 0x35, 0xe9, 0x7a, 0x5e,
        0x07, 0x8a, 0x5a, 0x0f, 0x28, 0xec, 0x96, 0xd5,
        0x47, 0xbf, 0xee, 0x9a, 0xce, 0x80, 0x3a, 0xc0, 
    ]).unwrap();
    // From blockstream.info -- note needs to be reversed!!
    let tlbtc_assetid = elements::AssetId::from_slice(&[
        0x49, 0x9a, 0x81, 0x85, 0x45, 0xf6, 0xba, 0xe3,
        0x9f, 0xc0, 0x3b, 0x63, 0x7f, 0x2a, 0x4e, 0x1e,
        0x64, 0xe5, 0x90, 0xca, 0xc1, 0xbc, 0x3a, 0x6f,
        0x6d, 0x71, 0xaa, 0x44, 0x43, 0x65, 0x4c, 0x14, 
    ]).unwrap();
    // from node
    let genesis_hash = elements::BlockHash::from_byte_array([
        0xc1, 0xb1, 0x6a, 0xe2, 0x4f, 0x24, 0x23, 0xae, 0xa2, 0xea, 0x34, 0x55, 0x22, 0x92, 0x79, 0x3b, 0x5b, 0x5e, 0x82, 0x99, 0x9a, 0x1e, 0xed, 0x81, 0xd5, 0x6a, 0xee, 0x52, 0x8e, 0xda, 0x71, 0xa7, 
    ]);

    // 1. Generate address
    let sk = SecretKey::from_slice(&[
        0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80,
        0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80,
        0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80,
        0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80,
    ]).unwrap();
    let secp = Secp256k1::new();
    let keypair = Keypair::from_secret_key(&secp, &sk);
    let pk = keypair.x_only_public_key().0;

    let descriptor: Descriptor<XOnlyPublicKey> = format!(
        "eltr({unspendable_key},sim{{pk({pk})}})"
    ).parse().unwrap();

    let tr = match descriptor {
        Descriptor::Tr(ref tr) => tr,
        _ => unreachable!(),
    };
    // FIXME file issue about returning Option<&T> rather than &Option<T>
    let taptree = tr.taptree().as_ref().unwrap();
    let leaves: Vec<_> = taptree.iter().collect();
    assert_eq!(leaves.len(), 1);

    let pol = match taptree {
        TapTree::SimplicityLeaf(ref pol) => pol,
        _ => unreachable!(),
    };

    let address = descriptor.address(&AddressParams::LIQUID_TESTNET).unwrap();
    assert_eq!(
        address.to_string(),
        "tex1p6cudwwh3djf84djcsy6f7u4ujw2r5k7743qlpvur5e44649gghtqy74pp0",
    );
    println!("Address: {address}");
    println!("Descriptor: {descriptor}");
    println!("Simplicity Policy: {pol}");
    println!("Simplicity Policy Program: {}", pol.commit().unwrap().encode_to_vec().as_hex());
    println!("Simplicity Policy CMR: {}", pol.cmr());

    // 2. (User needs to send coins to this address then hardcode a new outpoint here)
    //
    // Sent 100000 LBTC to address tex1p6cudwwh3djf84djcsy6f7u4ujw2r5k7743qlpvur5e44649gghtqy74pp0 with transaction a06dc55a3d5225d3d7ea21aa5792598c0ed239ad549945782438a78daf85b31b.
    // From blockstream.info we see this is output 1.
    //
    // Faucet return address is tlq1qqd0qxdqsag3t63gfzq4xr25fcjvsujun6ycx9jtd9jufarrrwtseyf05kf0qz62u09wpnj064cycfvtlxuz4xj4j48wxpsrs2
    let funding_tx_bytes = Vec::<u8>::from_hex("020000000101e7943a675cef06adc13680aa405e5d3d19a41af8d6c474055d173f30fbe286250100000000fdffffff030a5336eabc4f6276ff483c68941d8485efd3388f8709acbcd051c549938f5198d608f242d31416761f1f19c47f3ebf1f43f34df12306919fb694764a2d0ac0edd82e02d48cb03114ac3d99b88b45628717f5974c03f8b18f155db204ce6dae99827c0016001465b0986a5820020c932e31e43e22d1015b9f081f01499a818545f6bae39fc03b637f2a4e1e64e590cac1bc3a6f6d71aa4443654c140100000000000186a000225120d638d73af16c927ab65881349f72bc93943a5bdeac41f0b383a66b5d54a845d601499a818545f6bae39fc03b637f2a4e1e64e590cac1bc3a6f6d71aa4443654c1401000000000000008b0000637e180000000247304402204a2d4beab53bdbf1cbf11119713c8b708bd4fc486b66270d4a20e50eed2678f802203b0a0670ba32061970182e60b69f9e89e11d952c4b5568854a0f8f1cb982e282012103e654d9b88b51eaa8e947bf488619d493bab8d0a8bac12075c058b3c2f526820700430100017023610cb1780d7a91c560bc1658f4d5cc4fe731050790a8437ab1d2b8fa02a8d49b43e7673b30e100fbd0c3cf5ba395f7cca51d72d08a22816ec9cb67f65ab1fd4e10603300000000000000013d7781013440d84e3b91282a22ab987d8f67458eccff420a2c2893b2fbf1ad6c99b32a55aef0a1adc6fa3cf0a3199a5d6046f5436f38385fcca2ed38751e544dc37dfdc5e1b87976d55cc94e05df024bdfd628bec56bf70327b5e93d84acd5e4d7dd7ebf4e0752c77bb1aafc4eb7a33e1c1b8e1bfd68800b3ae41f6c5779d5e8695e99cfe7b4a7f6db9e293d2e62e3228ebde857833fa9c4abd16d902ba9f085e99ef4a2c41a736a899d2e54a07acdc77f071e6f025dd5147e2c2726ee0affdcbf33238c8a2529409565d8e1a0f5053de0bf436bc61c33cddd558505185300764e6f999c1284581703c7f5fdc7fca82cb2723600c40caf73c55d53b99d4c43db84bc282f7f37072beb56fdf803d451db15d38cd2e70b28df8b2ba741a7e94ae5a83c9df869656024d955214ea272f5a246369a4a3cfbdee926a4fd8785c8f23d9425d39b275abff98d91d6d59a61bc355736d9fe054704c3cfde1a7a7b9aa60d2e85da7ccc165f31a47eff27daa46112665aff47c549c4fa4246f81f13cfca6c17f2f8ee43d4300d3e6577f342e107b9e1a524040c152f08bb6a6e01fcc849f32a289f35226f944c593f6ba37ec57c37ff4006c657df00a5d330ae85bd2f599b554d63f9bc259a7ec5c9b4c8740c1e9a270a75628529228c16bba4d63d5ab6b890d5c9ad4e6c3437e7e25f410749ad9b673cfbbddcd7fd2d86593364e6f19718c3dd8fa8fdeb6c1119b98b22cd1f92aeb30482c8a86325c2ebb6c19f0f32e7afa3babef390102ec63a029eb6c4e669d47ad7c6b12064097160d83e446265eb36a6e8f067c7035feeea7eb4d4bb24e6232460dbbe25506bd0541650bad1c068bc221c83e05bd2a0cb5ed0f31fe309cad5d9990e170fa2eb33bd2dc2f007f6f609bb951a116e481a45065b714a63f7a84b257cbadd05e568b46a18059122c5f0096726ebdff0a3537332e180b3ebc98d190943883bad77f221d0efe589aab4a227061677a6bdd69815e4a363408c7c23d0a2bf383c4b2cb4761d775e1ab1c9d60457df7049287cf05faf7c09840f4d8ddf948449ddf029cee86c704f793cd785f1973b417833ad271093356e8954d6cfd90f3578d5ce7d4bcbce2e19a34a579e92b2960d5ea4cc13abed1a5247c0c09b22ebc985d3e37a03b9c088c4c69d2e57ea5bccd2091ced9a7459b6f3c6e32cccbfafb30e77e2fd0946f2cf1149cd6ad96a6c1765e0eb1acf466b9bdcf151c37bc0e4484dd4b3bd76494b6567f04d305c8ca7dc1a7a080eb2a242eadaddb70a7f4fbae81895b4f1e198083e4642139227ab6baa389905c8168bd9b4fbda443e5d2440c7604c0f4a118e9cd563d631b4971ce25c0c2ad71c64e7dcb68f41776728a82f10eab56c5f2347cc9bed850b4a72232269f9cd93c813878d5e60d9c55012de727ddb1dcf4b71b6bfaa70c9d4deddd94ef59d9aab013741f1a2292f5b0c07761b07ec43e8ea3f33f3044c668ec90c675f9d534003bfb6c07e47c1fc7a76394a911af09457ee96b8a1a0d0a78127af35fff733e8729c904da61d8c10050d8e74ca7232ceb0e892588ce326a3f4c95e6c61bba4ca3133b7db7201c22727fa7df615113ea7f6821d0c66e5b50d1de0f09a2274ab7942d5521dae872c215526097406f85d350c0cc91f784c78464f048b8a9ad14bb6411a019311cddb32a3b6ad9b25e05d3cc0dde7c9f9ad3ee694107cdacf32873463902f184f9adc42efd5fbfaed6114f2d3d05536e34bcd63d4b5a9bf3408fa6e90d1d1f93d19a097ce2ea94b65d1c9af93f9e7b440fa67635d6e1e183e791f7dd2dc6f98e1d5cb207ad0eee205626842e4a5b4472a0061f4c740be9c8cd26a28bef9edc7c49de1b5dbf1f9e8373b7f58a6483a2fb648b8957838133a134373b3225214476f070f778723099f5694b369a334e9a49afb76af015fb097e183a171c2300e1af1d7f85ba8f125573351d2ee14f868ba79f63b20d046ce5f2219c1f726c588aba202018378cc138c398b1738a87a34e9e6c7dd759e99e59205c0d29b62d7aab7bad21121b136d187e905fbf18bb9c10b779b1b367e39033f6d31a4d50f74bdafe1ad03ce4b870b0b73022e93edc3039e2c74dba164f29fdf766231c9dda375ea7146b12acde13c2194d056d894436ced070f9802f43a6487ea5d9ae7b710365cdf045b8d42f0897fb3082372bde27bd8982bf1389fe532e0224148e3fc95bb7287d22d988d5b9bc246f17e3c2db5d3543e8d34348eb671f1d2f9a8058f86e83720912d8f771f3a0fd8c45fca5a7dacd423253105e788b38c4ce881536532888447501cc1cd7d0c243679ff0df8456b1f28f0f26817252d9b55bd44140d04bc73181547053b2de1519c6a5e0dae43f9437977eca018fb0cc892d6c85e43db9fb146425cef0bbf3361de0e757a03e5353f99b518c9b8d1a3b5b736763aa2f6b5367f55013a3f63ded429baaf6bff4dd9dddc35b3bf2a8610477ea4b59f04c248a9ef2595ea9128cc1446c0bd018d4c9d882ece0a2335114a841248e2a0671c0d078b0505f8f74b13d00ed3a8f032c94ba9e2dba569ddc30519f48f174528aa7f5796b49c86535144a666cee25dfac9efe941255fa9bda113db151d4eb8961289133c71abfad165f5824d9a3380a182f7aab499af3aaa197707df40a06f04bfa214d6c35dae72c14d218e313937f8de8a6fb132726f46f3e07789d3cb35406198c3a466991443bd96ae51e26ba2d12aeb7a63ebc29632fbb5546d5258c4e0595c13ef160992444137e7977f53c503d0b234c22a7fa3638c6a0f9814563b21d5c539f0a20db830ab3a49dbde7f1321bf5a754d3167fce6fa77ea62b790d53875d4e7b298c9efc9b1ca632d03d3fa329dbc3984208964fe3d098a60b03eed5ec987bb9d4c8cb556a2296f947899c82f78ed672e1c71bfe3d0610ba9d8251369a8862703f8f75a2d4ced9b4d97cbba281a3ea02134406de6aa7c2ed11ea8447647b2a1dbb3c1e625f8ab1e1c5725f2321e7fff53b8bf3fa73b1f88068f194f27ffe2c229032c29d51e7745b25bb012bd2952570c79f3879d5fb36624b3667fea1cbfb2b052b4c34e01c4e15ecf9b08cb0b2db7fac2a69efd96c3ad2c7d13a51acf12c85bfd3b8a18c944d9d4065fda6dcf693762820afc2bdba3e8285004d95ae06ab9033c37c1987c5abc7b9b3d62ef8e44b70c3542c8e2f7e2591d2382f8d79b9ba30c27a4732f3ee5631f0254cdb9d8d36bf1825dae5e1d446c38cbc0b52f47a7ac651b84340dfb384c4387fbd502294d75b8852626705521690eed1510d7f7154408e3c74f23d193cb4f1fce78bb41edfd9ea37be7562e553b56f07c017629c34215c0521115aade951d2eeed7f7e733434324e3fa4673674cd14fff69bb3217b4849319be6bd1886b8dc2ed5f330d7ef8105e0877a30b0a35720eb30e16ce91246421ced80dec92cbba23c9b9b5367066cbcaa331505285c6252e88e20cead99ab3809e7cb139faae214c9f959e859944b73441170d7e3f9d568bed796fda638c211852b54eeb68d16b87599ea2050c15e5900d919252375a16f1edc20baa25d949454219039db5eb2c82bfff797a1fd6f6c03d00b3e74126a5e1c04feee4c9f4ab2ff24bddc521c62f3400031016395b99964e2215e9ee5295da4e49cb589a34c23ead9328f059069e30ef4439634c2ede670024dc5ec93c68b2275327a324e9ff7de4f2b23438d760fbc7ccdbbf4ae3f248515776c8c005dd0092e1aec669340c39250f5ee26f2ed659e133ded55c1930ddfd6ee5d8bf7688ef3dedc63097ca0984eeeb1296677655afc44551d4367e7fd1ce291eebcc15d127616b9c6db3da43152b7456809d154b70703073a6458ffd0e5b45ac3750b5c84f39185cdb64747df9388352eda4c73cf1bc137a212d185b364fd15c4aa97c660787192b2f3423b14da9a3f4caa28f7b70ea0c149ac8fe8a3c39c530d875bfab459d083c2010362fa464fd263c390a5185444fb37df63a8b827ed54d8f60e90cd1c47bd89243cb2c1fbf4d27426c43d53ab6cbf8509a3382719ee1a89c7804e5f8233f3c5732c0bbf24242a02af0dd9a7202f29fb870eb020ff57f9e6fa0e21737ca238e414666f67f8fe51b8abdf1e26f621b42aa5f87833f03ecfb9724a76472aaa06b557deb239e93598d2c7eeec99e608090cda9258acb7efa37ff8dbc48bd2d34a4f23f80417fd4e7a8f4b2cd2a130a186d16e876de9d8d2c6f12623bfeec18e4a976e6fcc5a7c51b4748edf5c02420e247384c2c4249520e2b7dd11152ebaf59fa92f784b08d34428c5c88a137d91d944c474770216294cb588b4b44dda317249288a78ce7caef8fd8314f747e00f448cbd0d948efeae472ba13eb6a3482b4bcd92c02b9adce05058b62cc7ebae09d100031eecc37088aeb548bb1bdf8473c16e99361afdd5c9a39f53763e4f1763109b5748541ee86ae678f691282e1091f2689adb61b3ae6ec2c6ea0f03556726d100b014a217ff0386c86bfafcff90ab95745c7b49505985ecff7b563bf6de1220738f05289063d65b46ca99dbc90d8c6731d1ba7e1b2b1c0ef8db61d3ccbec7b075e7129e2f602a8db4d105305015f9e17f812e421a83b6ac53bab793d7ea37976a00c4fb24091ca6e5712306efbb86a80aa4234e77f4f0a039c4587cda79fe599273f8a29c96460ce16fbbb797ccc28e5bc4eab6b7e888c3bade392a94a4ec450d5bd42c5a647b80961d07278bcb1340fde8a61c4a3f851d48fbf05cdd225bab64b44b659b15790e10098fbb12d1976fae3c1ea88dac477896f3ac65b7801c3f7f70fb50f7f7fd41a53021083c58b99dfd91d8a11d7643c9b7c17be8e1ad299d8920cce373c1676adde915588d04683b3dc08d60ce80f07f2953ae3fd1adf0dffc73d2200c5ee45ff01a4b42174d5d07278bcb56ab2d39c809049a087f4017d46a53fdbfe8771a0464d1cdc76b35abd2bc84c80692c02b13f556e0147e47c0e35b7ca037cf6e156af99e3e26ab8dd0a6de320367e2490df517bcc607fd26bc48a11d6b5981c5714e5c780c73ec3315205fa65b3ff1034d5d6513696f86fdda0f5940c76d210d5ac90da9d706f8732ff25da6bf6e8a5af9be81a46f3760e54de05580ff72127fc108020358d381c4316ddd817ef7128f68eb92786d78b3baae380b46c6f6f0b1039338bd8188c17d25a79bde3b024ecbaa6ee08c5507e8fc2b1720df4e2af442554a7ccc17a8e6ba25d50614d56460be6c02ab3d68b8e13b8947a41b5b8bcfb5cbc0f6fbf87e8edd6991633d3134b39bb73e307943656637174f49e48edd0b5eec02addd115c5ddcad0ff9245c573b88c179bf40b8642211cf37e140ccaa1f8dde7200382447404d1cbfd6052fca7222759ddfe5420e10f1e8008a5074cac6b9ed3115e9d75374265cfb809b1a6487a366064f5195024dce64196b354ce44bffe7be6109b005c824fe141a7a4ac4503addbf5fac1075591b51103a6fadf99a61a8469f4d4099fa5189a525cc2be4ceb5d21698fe36dc9f970428443d24ac6f09a8f5067c493c801abf481cfa515f7f7697f16d2f6dc3643b901060392aa74515331cf10d78b24b560e3ace04f5e76a35b9039e29dad22d659eabee1affac8ebac8539679402eabb49c66a39879b8ac5bf37e56f26afb5b2465fb733b6bc62e9b1127b1f2039de459e235292a0a2d334ca72a8eb6ac31be6bf10edca0b5a8a02a73f3eb98aba729ff399727072032e67686e7f84cc64b4550be69fb1617e0292be69ba5b6ebaf30689b06df30d4be903a69e2bb57f2344f43f7c9339806828674b2a64faef0f7decc16721f7f1e0d9ef7c021d65f6e0cc12dcafe2825aeb7ef0d80cdf9d9cf57f02273cc500000000").unwrap();
    let funding_tx: elements::Transaction = elements::encode::deserialize(&funding_tx_bytes).unwrap();
    assert_eq!(
        funding_tx.txid().to_string(),
        "a06dc55a3d5225d3d7ea21aa5792598c0ed239ad549945782438a78daf85b31b",
    );
    
    let faucet_addr: elements::Address = "tlq1qqd0qxdqsag3t63gfzq4xr25fcjvsujun6ycx9jtd9jufarrrwtseyf05kf0qz62u09wpnj064cycfvtlxuz4xj4j48wxpsrs2".parse().unwrap();

    let mut tx = elements::Transaction {
        version: 2,
        lock_time: elements::LockTime::ZERO,
        input: vec![
            elements::TxIn {
                asset_issuance: elements::AssetIssuance::null(),
                is_pegin: false,
                previous_output: "a06dc55a3d5225d3d7ea21aa5792598c0ed239ad549945782438a78daf85b31b:1".parse().unwrap(),
                script_sig: elements::Script::new(),
                sequence: elements::Sequence::MAX,
                witness: elements::TxInWitness::default(),
            }
        ],
        output: vec![
            elements::TxOut {
                asset: elements::confidential::Asset::Explicit(tlbtc_assetid),
                nonce: elements::confidential::Nonce::Null,
                value: elements::confidential::Value::Explicit(99500),
                script_pubkey: faucet_addr.script_pubkey(),
                witness: elements::TxOutWitness::default(),
            },
            elements::TxOut {
                asset: elements::confidential::Asset::Explicit(tlbtc_assetid),
                nonce: elements::confidential::Nonce::Null,
                value: elements::confidential::Value::Explicit(500),
                script_pubkey: elements::Script::default(),
                witness: elements::TxOutWitness::default(),
            }
        ],
    };

    let txenv = ElementsEnv::new(
        Arc::new(tx.clone()),
        vec![ElementsUtxo {
            asset: funding_tx.output[1].asset,
            value: funding_tx.output[1].value,
            script_pubkey: funding_tx.output[1].script_pubkey.clone(),
        }],
        0,
        pol.cmr(),
        tr.spend_info().control_block(
            &(
                leaves[0].1.encode(),
                LeafVersion::from_u8(0xbe).unwrap(), // from elements source
            ),
        ).unwrap(),
        None,
        genesis_hash,
    );
    let (txhash, txhashlen) = call_jet::call_jet(&txenv, Elements::SigAllHash, &[]);
    assert_eq!(txhashlen, 256);
    println!("Sighash: {}", txhash.as_hex());

    let sig = secp.sign_schnorr(&Message::from_digest_slice(&txhash).unwrap(), &keypair);
    let schnorr_sig = SchnorrSig {
        sig,
        hash_ty: SchnorrSighashType::All, // ignored
    };

    let mut sig_map = HashMap::new();
    // nb this "all_zeros" thing, and the ignored hash type of the schnorrsighash type,
    // are totally undocumented and weird
    sig_map.insert((pk, TapLeafHash::all_zeros()), schnorr_sig);
    Descriptor::<XOnlyPublicKey>::satisfy(
        &descriptor,
        &mut tx.input[0],
        &sig_map,
    ).unwrap();

    println!("Satisfied tx:");
    println!("{}", elements::encode::serialize_hex(&tx));


    Ok(())
}