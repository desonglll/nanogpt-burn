use burn::nn::{Initializer, Linear, LinearConfig, Dropout, DropoutConfig};
use burn::{
    config::Config,
    module::Module,
    tensor::{activation, backend::Backend, Bool, Tensor, Int},
};
// use libm::sqrtf;

#[derive(Config)]
pub struct HeadConfig {
    batch_size: usize, 
    /// Context size 
    block_size: usize, 
    n_embd: usize, 
    head_size: usize, 
    dropout: f64, 
    /// The type of function used to initialize neural network parameters
    #[config(
        default = "Initializer::KaimingUniform{gain:1.0/libm::sqrt(3.0), fan_out_only:false}"
    )]
    pub initializer: Initializer,
}

impl HeadConfig {
    pub fn init<B: Backend>(&self, device: &B::Device) -> Head<B> {
        // compute the weight matrix 
        let tril: Tensor<B, 3, Int> = Tensor::ones(
            [self.batch_size, self.block_size, self.block_size], 
            device, 
        ).tril(-1);
        let tril = tril.equal_elem(0); 

        Head {
            key: LinearConfig::new(
                self.n_embd, 
                self.head_size, 
            ).with_initializer(self.initializer.clone())
            .init(device),
            query: LinearConfig::new(
                self.n_embd, 
                self.head_size, 
            ).with_initializer(self.initializer.clone())
            .init(device),
            value: LinearConfig::new(
                self.n_embd, 
                self.head_size, 
            ).with_initializer(self.initializer.clone())
            .init(device),
            tril, 
            dropout: DropoutConfig::new(self.dropout).init(),
        }
    }
}

#[derive(Module, Debug)]
pub struct Head<B: Backend> {
    query: Linear<B>, 
    key: Linear<B>, 
    value: Linear<B>, 
    tril: Tensor<B, 3, Bool>, 
    dropout: Dropout, 
}

impl<B: Backend> Head<B> {
    /// Single head attention 
    /// input of size (batch, time-step, channels)
    /// output of size (batch, time-step, head size)
    pub fn forward(&self, x: Tensor<B, 3>) -> Tensor<B, 3> { 
        // (B,T,hs)
        let k = self.key.forward(x.clone()); 
        // (B,T,hs)
        let q = self.query.forward(x.clone()); 

        // (B, T, hs) @ (B, hs, T) -> (B, T, T)
        let wei = (q * k.clone().transpose()) / ((k.dims()[2] as f32).sqrt()); 
        // (B, T, T)
        // ref https://docs.rs/burn/0.9.0/burn/tensor/struct.Tensor.html#method.mask_fill
        // A value too low might result in NaN
        let wei = wei.mask_fill(self.tril.clone(), -1.0e4); 
        // (B, T, T)
        // ref https://docs.rs/burn/0.9.0/burn/tensor/activation/fn.softmax.html
        let wei = activation::softmax(wei, 2); 
        let wei = self.dropout.forward(wei);
        // (B,T,hs)
        let v = self.value.forward(x); 
        // (B, T, T) @ (B, T, hs) -> (B, T, hs)
        let out = wei * v;  
        out 
    }
}

#[derive(Config)]
pub struct MultiHeadAttentionConfig {
    pub n_layer: usize, 
    pub n_head: usize, 
    pub head_size: usize, 
    pub n_embd: usize,
    pub dropout: f64,  
}

impl MultiHeadAttentionConfig {
    pub fn init<B: Backend>(&self, device: &B::Device, head_config: &HeadConfig) -> MultiHeadAttention<B> {
        let layers = (0..self.n_layer)
            .map(|_| head_config.init(device))
            .collect::<Vec<_>>();

        MultiHeadAttention {
            proj: LinearConfig::new(
                self.head_size * self.n_head, 
                self.n_embd, 
            ).init(device),
            dropout: DropoutConfig::new(self.dropout).init(), 
            heads: layers,
        }
    }
}

#[derive(Module, Debug)]
pub struct MultiHeadAttention<B: Backend> {
    proj: Linear<B>, 
    dropout: Dropout, 
    heads: Vec<Head<B>>, 
}

impl<B: Backend> MultiHeadAttention<B> {
    pub fn forward(&self, x: Tensor<B, 3>) -> Tensor<B, 3> {
        let mut inputs = Vec::new(); 
        for head in self.heads.iter() {
            inputs.push(head.forward(x.clone())); 
        }
        let x = Tensor::cat(inputs, 2); 
        let x = self.proj.forward(x); 
        let x = self.dropout.forward(x); 
        x  
    }
}