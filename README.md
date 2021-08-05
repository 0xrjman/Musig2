# Musig2



## Start

use `RUST_LOG=info cargo run`, start up three terminal



## sign

choose one terminal,input `sign "message"`, terminal display:

~~~
peerid start sign, send round1:PeerId("12D3KooWFn5ZBgcjaoVCR8snGDsZ1LKbAEwkGie48kxhd51sE37N")
recv round1 peerid:PeerId("12D3KooWHSyDS8GLkD7j5N7JmZirAuTyWQ9GkLdj7E9uXCBKH9Gc")
recv round1 peerid:PeerId("12D3KooWE3mUPfQstUwAgvdd1EL59u3EvyrpFx3DKg9L4wyXDKvv")
recv round2 peerid:PeerId("12D3KooWE3mUPfQstUwAgvdd1EL59u3EvyrpFx3DKg9L4wyXDKvv")
peerid send round2:PeerId("12D3KooWFn5ZBgcjaoVCR8snGDsZ1LKbAEwkGie48kxhd51sE37N")
recv round2 peerid:PeerId("12D3KooWHSyDS8GLkD7j5N7JmZirAuTyWQ9GkLdj7E9uXCBKH9Gc")
sign finish, R:Secp256k1Point { purpose: "combine", ge: PublicKey(3e1bb4e22fcb53d25ef7d8ada87eca19e82290b9fcce5b7e65b6f81f9b8a9a7b2e57a89fc1c826b144fbc5e719b55bde312257052cdefe1b92c09980064be992) }, S:Secp256k1Scalar { purpose: "add", fe: SecretKey(eb153b69b05026f6c5dedb1e1267622fd39ada47403f153907fbe2cec89bf08e) }
~~~



## verify

choose one terminal,input `verify`, terminal display:

~~~
verify result:true
~~~



