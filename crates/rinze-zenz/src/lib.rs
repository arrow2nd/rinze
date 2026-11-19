//! zenz (GGUF) で、左文脈と読みを与えたときの各候補の対数尤度を計算する。
//!
//! プロンプトは `[<CTX> 左文脈末尾 40 字] <IN> 読み (カタカナ) <OUT>` で、BOS を付けて 1 系列で decode する。
//! その KV を各候補の系列へ複製し、全候補のトークンを 1 バッチで decode する。
//! 点数は候補トークン列の対数尤度の和で、EOS は含めない。
//!
//! [`Zenz::generate`] は同じプロンプトから表記をビーム探索で生成する。

use std::error::Error;
use std::ffi::{CString, c_char};
use std::num::NonZeroU32;
use std::path::Path;
use std::pin::pin;

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::params::kv_overrides::ParamOverrideValue;
use llama_cpp_2::token::LlamaToken;

const TAG_INPUT: char = '\u{EE00}';
const TAG_OUTPUT: char = '\u{EE01}';
const TAG_CONTEXT: char = '\u{EE02}';

const LEFT_CONTEXT_CHARS: usize = 40;
const N_CTX: u32 = 1024;
const N_SEQ_MAX: u32 = 64;

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub struct Zenz {
    _backend: LlamaBackend,
    model: &'static LlamaModel,
    ctx: LlamaContext<'static>,
    batch: LlamaBatch<'static>,
    stops: Vec<bool>,
}

/// 生成した表記と、その対数尤度。
#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    pub text: String,
    pub score: f64,
}

struct Beam {
    tokens: Vec<LlamaToken>,
    score: f64,
    seq: i32,
    logits: i32,
}

impl Zenz {
    /// llama.cpp のバックエンドは 1 プロセスで 1 度しか初期化できないので、`Zenz` も 1 プロセスに 1 つだけ作れる。
    pub fn load(model_path: &Path, threads: i32) -> Result<Self> {
        let mut backend = LlamaBackend::init()?;
        backend.void_logs();

        let mut params = pin!(LlamaModelParams::default().with_n_gpu_layers(0));
        // zenz の gguf は tokenizer.ggml.pre に azooKey 版 llama.cpp にしか無い名前を持つ。
        // 中身は GPT-2 と同じ正規表現なので、上流の名前 "gpt-2" に上書きしないと読み込めない
        let mut value = [0 as c_char; 128];
        for (dst, src) in value.iter_mut().zip(b"gpt-2") {
            *dst = *src as c_char;
        }
        params.as_mut().append_kv_override(
            &CString::new("tokenizer.ggml.pre")?,
            ParamOverrideValue::Str(value),
        );
        let model: &'static LlamaModel = Box::leak(Box::new(LlamaModel::load_from_file(
            &backend, model_path, &params,
        )?));

        let cparams = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_batch(N_CTX)
            .with_n_ubatch(512)
            .with_n_seq_max(N_SEQ_MAX)
            .with_n_threads(threads)
            .with_n_threads_batch(threads)
            .with_kv_unified(true)
            .with_flash_attention_policy(0)
            .with_no_perf(true);
        let ctx = model.new_context(&backend, cparams)?;
        let vocab = model.vocab();
        // zenz-v3.2 は出力のあとに EOS を出さず、次のタグ (U+EE00 台の私用文字) を生成し続ける。
        // UTF-8 で 0xEE は U+E000..=U+EFFF の先頭バイトにしか現れないので、それを含むトークンで打ち切る
        let stops = vocab
            .tokens()
            .map(|t| {
                vocab.is_eog(t)
                    || vocab.is_control(t)
                    || vocab.token_to_piece(t, true, None).contains(&0xEE)
            })
            .collect();
        Ok(Self {
            _backend: backend,
            model,
            ctx,
            batch: LlamaBatch::new(N_CTX as usize, 1),
            stops,
        })
    }

    /// `reading` はひらがなの読み (送りがなを含む)。`candidates` も送りがなを含めた表記で渡す。
    /// 戻り値は `candidates` と同じ順の点数で、大きいほど尤もらしい。
    pub fn score(
        &mut self,
        left_context: &str,
        reading: &str,
        candidates: &[String],
    ) -> Result<Vec<f64>> {
        let prompt = self.tokenize(&build_prompt(left_context, reading), true);
        let cands: Vec<Vec<LlamaToken>> =
            candidates.iter().map(|c| self.tokenize(c, false)).collect();
        let p = prompt.len() as i32;

        self.ctx.clear_kv_cache();
        self.batch.clear();
        for (i, &t) in prompt.iter().enumerate() {
            self.batch.add(t, i as i32, &[0], i + 1 == prompt.len())?;
        }
        self.ctx.decode(&mut self.batch)?;
        let first = self.ctx.get_logits_ith(self.batch.n_tokens() - 1).to_vec();

        let mut scores = Vec::with_capacity(cands.len());
        for group in cands.chunks((N_SEQ_MAX - 1) as usize) {
            for s in 1..=group.len() as i32 {
                self.ctx.clear_kv_cache_seq(Some(s as u32), None, None)?;
                self.ctx.copy_kv_cache_seq(0, s, Some(0), Some(p as u32))?;
            }
            self.batch.clear();
            let mut positions: Vec<Vec<i32>> = Vec::with_capacity(group.len());
            for (g, toks) in group.iter().enumerate() {
                let seq = g as i32 + 1;
                let mut v = Vec::new();
                for (k, &t) in toks.iter().enumerate() {
                    v.push(self.batch.n_tokens());
                    self.batch.add(t, p + k as i32, &[seq], true)?;
                }
                positions.push(v);
            }
            if self.batch.n_tokens() > 0 {
                self.ctx.decode(&mut self.batch)?;
            }
            for (toks, idx) in group.iter().zip(&positions) {
                let Some(&head) = toks.first() else {
                    scores.push(0.0);
                    continue;
                };
                let mut sum = log_prob(&first, head);
                for (&bi, &next) in idx.iter().zip(&toks[1..]) {
                    sum += log_prob(self.ctx.get_logits_ith(bi), next);
                }
                scores.push(sum);
            }
        }
        Ok(scores)
    }

    /// `reading` (ひらがな、送りがなを含む) の表記を、対数尤度の高い順に最大 `width` 件生成する。
    /// 幅 `width` のビーム探索で、出力の終わりまで生成できたものだけを返す。
    pub fn generate(
        &mut self,
        left_context: &str,
        reading: &str,
        width: usize,
    ) -> Result<Vec<Generated>> {
        let width = width.clamp(1, (N_SEQ_MAX as usize - 1) / 2);
        let prompt = self.tokenize(&build_prompt(left_context, reading), true);
        let p = prompt.len() as i32;
        let max_tokens = reading.chars().count() + 4;

        self.ctx.clear_kv_cache();
        self.batch.clear();
        for (i, &t) in prompt.iter().enumerate() {
            self.batch.add(t, i as i32, &[0], i + 1 == prompt.len())?;
        }
        self.ctx.decode(&mut self.batch)?;
        let mut alive = vec![Beam {
            tokens: Vec::new(),
            score: 0.0,
            seq: 0,
            logits: self.batch.n_tokens() - 1,
        }];
        let mut done: Vec<(Vec<LlamaToken>, f64)> = Vec::new();

        for step in 0..=max_tokens {
            let mut next: Vec<(usize, LlamaToken, f64)> = Vec::new();
            for (b, beam) in alive.iter().enumerate() {
                let logits = self.ctx.get_logits_ith(beam.logits);
                let lse = log_sum_exp(logits);
                for (token, logit) in top_k(logits, width) {
                    let score = beam.score + logit as f64 - lse;
                    if self.stops[token.0 as usize] {
                        if !beam.tokens.is_empty() {
                            done.push((beam.tokens.clone(), score));
                        }
                    } else if step < max_tokens {
                        next.push((b, token, score));
                    }
                }
            }
            done.sort_by(|a, b| b.1.total_cmp(&a.1));
            done.truncate(width);
            next.sort_by(|a, b| b.2.total_cmp(&a.2));
            next.truncate(width);
            if done.len() == width {
                let worst = done[width - 1].1;
                next.retain(|c| c.2 > worst);
            }
            if next.is_empty() {
                break;
            }

            let base = 1 + (step % 2) as i32 * width as i32;
            self.batch.clear();
            let mut grown = Vec::with_capacity(next.len());
            for (j, &(b, token, score)) in next.iter().enumerate() {
                let parent = &alive[b];
                let seq = base + j as i32;
                self.ctx.clear_kv_cache_seq(Some(seq as u32), None, None)?;
                self.ctx.copy_kv_cache_seq(parent.seq, seq, None, None)?;
                self.batch
                    .add(token, p + parent.tokens.len() as i32, &[seq], true)?;
                let mut tokens = parent.tokens.clone();
                tokens.push(token);
                grown.push(Beam {
                    tokens,
                    score,
                    seq,
                    logits: self.batch.n_tokens() - 1,
                });
            }
            for beam in alive.iter().filter(|b| b.seq != 0) {
                self.ctx
                    .clear_kv_cache_seq(Some(beam.seq as u32), None, None)?;
            }
            self.ctx.decode(&mut self.batch)?;
            alive = grown;
        }

        let vocab = self.model.vocab();
        Ok(done
            .into_iter()
            .filter_map(|(tokens, score)| {
                let bytes: Vec<u8> = tokens
                    .iter()
                    .flat_map(|&t| vocab.token_to_piece(t, false, None))
                    .collect();
                let text = String::from_utf8(bytes).ok()?.replace('　', " ");
                Some(Generated { text, score })
            })
            .collect())
    }

    fn tokenize(&self, text: &str, add_bos: bool) -> Vec<LlamaToken> {
        let text = text.replace(' ', "　").replace('\n', "");
        self.model.vocab().tokenize(text.as_bytes(), add_bos, false)
    }
}

fn log_prob(logits: &[f32], target: LlamaToken) -> f64 {
    logits[target.0 as usize] as f64 - log_sum_exp(logits)
}

fn log_sum_exp(logits: &[f32]) -> f64 {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
    max + logits
        .iter()
        .map(|&v| (v as f64 - max).exp())
        .sum::<f64>()
        .ln()
}

fn top_k(logits: &[f32], k: usize) -> Vec<(LlamaToken, f32)> {
    let mut items: Vec<(LlamaToken, f32)> = logits
        .iter()
        .enumerate()
        .map(|(i, &v)| (LlamaToken(i as i32), v))
        .collect();
    let k = k.min(items.len());
    items.select_nth_unstable_by(k - 1, |a, b| b.1.total_cmp(&a.1));
    items.truncate(k);
    items
}

fn build_prompt(left_context: &str, reading: &str) -> String {
    let mut s = String::new();
    let chars: Vec<char> = left_context.chars().collect();
    if !chars.is_empty() {
        s.push(TAG_CONTEXT);
        s.extend(&chars[chars.len().saturating_sub(LEFT_CONTEXT_CHARS)..]);
    }
    s.push(TAG_INPUT);
    s.extend(reading.chars().map(to_katakana));
    s.push(TAG_OUTPUT);
    s
}

fn to_katakana(c: char) -> char {
    if ('ぁ'..='ゖ').contains(&c) {
        char::from_u32(c as u32 + 0x60).unwrap_or(c)
    } else {
        c
    }
}
