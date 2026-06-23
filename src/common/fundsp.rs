use fundsp::audionode::AudioNode;
use fundsp::combinator::An;
use fundsp::net::Net;

pub fn to_net<F: AudioNode + 'static>(fx: An<F>) -> Net {
    Net::wrap(Box::new(fx))
}
