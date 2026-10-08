/// The route classes used to select market pricing tiers.
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum PricingClass {
    NoRoute,
    Unclassified,
    Titan,
    JupiterShared,
    Pro,
    Authorized,
    Jupiter,
    PeerMarketMaker,
    RecognizedSigner,
    RouterSigner,
    SecondaryRouter,
    StandardQuote,
    TitanQuote,
    Settler,
    ProWithAuxiliarySigner,
    Sun,
}
