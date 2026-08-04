//! VRAM Headroom Calculator - shows what models fit in available VRAM.
//!
//! Estimates based on common quantization sizes and context length overhead.
//! This is what r/LocalLLaMA users desperately want in a monitoring tool.

use eframe::egui;
use crate::app::App;
use crate::gui::{theme, widgets};

/// Known model sizes at various quantization levels (in GB).
/// Format: (name, param_count, q4_gb, q5_gb, q8_gb, fp16_gb)
const MODELS: &[(&str, &str, f64, f64, f64, f64)] = &[
    ("Llama 3.1 8B",    "8B",   4.7,   5.5,   8.5,   16.0),
    ("Mistral 7B",      "7B",   4.4,   5.1,   7.7,   14.5),
    ("Qwen 2.5 14B",    "14B",  8.2,   9.8,   14.8,  28.0),
    ("Llama 3.1 70B",   "70B",  40.0,  48.0,  72.0,  140.0),
    ("Qwen 2.5 72B",    "72B",  41.0,  49.0,  74.0,  144.0),
    ("Mixtral 8x7B",    "47B",  26.0,  31.0,  48.0,  94.0),
    ("Llama 3.1 405B",  "405B", 228.0, 274.0, 415.0, 810.0),
    ("DeepSeek V3",     "671B", 377.0, 453.0, 688.0, 1342.0),
    ("Qwen3 Coder 80B", "80B",  45.0,  54.0,  82.0,  160.0),
];

const BYTES_PER_GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Estimate KV cache size in GB for a given context length and model shape.
fn kv_cache_gb(context_len: usize, num_layers: usize, head_dim: usize, num_kv_heads: usize) -> f64 {
    // KV cache = 2 (K and V) * layers * kv_heads * head_dim * context * 2 bytes (FP16)
    let bytes = 2.0 * num_layers as f64 * num_kv_heads as f64 * head_dim as f64 * context_len as f64 * 2.0;
    bytes / BYTES_PER_GB
}

/// Rough KV cache estimate based on param count string.
fn estimate_kv_cache(params: &str, context: usize) -> f64 {
    // Rough heuristic: bigger models have more layers/heads
    match params {
        "7B" | "8B" => kv_cache_gb(context, 32, 128, 8),
        "14B" => kv_cache_gb(context, 40, 128, 8),
        "47B" => kv_cache_gb(context, 32, 128, 8), // MoE
        "70B" | "72B" | "80B" => kv_cache_gb(context, 80, 128, 8),
        "405B" => kv_cache_gb(context, 126, 128, 8),
        "671B" => kv_cache_gb(context, 61, 128, 8), // MoE, fewer layers
        _ => 0.5, // fallback
    }
}

/// Free VRAM on a single card, in bytes. Never underflows.
fn free_bytes(used: u64, total: u64) -> u64 {
    total.saturating_sub(used)
}

/// Aggregate free VRAM across all detected GPUs.
///
/// Returns `(largest_single_card_free, combined_free)` in bytes. A model must
/// fit in the largest single card to run unsplit; the combined figure is only
/// meaningful for runtimes that shard layers across devices.
fn aggregate_free(gpus: &[(u64, u64)]) -> (u64, u64) {
    let largest = gpus
        .iter()
        .map(|&(used, total)| free_bytes(used, total))
        .max()
        .unwrap_or(0);
    let combined = gpus
        .iter()
        .fold(0u64, |acc, &(used, total)| {
            acc.saturating_add(free_bytes(used, total))
        });
    (largest, combined)
}

fn bytes_to_gb(bytes: u64) -> f64 {
    bytes as f64 / BYTES_PER_GB
}

/// Does `weights_gb` plus KV cache fit in `free_gb`?
fn fits(weights_gb: f64, kv_gb: f64, free_gb: f64) -> bool {
    weights_gb + kv_gb < free_gb
}

/// Context-length hint derived from leftover VRAM after weights + 4k KV cache.
fn max_context_label(headroom_gb: f64) -> &'static str {
    if headroom_gb > 4.0 {
        "32k+"
    } else if headroom_gb > 1.0 {
        "8k"
    } else {
        "4k"
    }
}

/// Verdict string for a model row, given Q4 size and current free VRAM.
fn verdict(q4_gb: f64, kv_gb: f64, largest_free_gb: f64, combined_free_gb: f64, gpu_count: usize) -> String {
    if fits(q4_gb, kv_gb, largest_free_gb) {
        let headroom = largest_free_gb - q4_gb - kv_gb;
        format!("Q4 fits ({}ctx)", max_context_label(headroom))
    } else if gpu_count > 1 && fits(q4_gb, kv_gb, combined_free_gb) {
        "Q4 fits (split)".into()
    } else {
        "Too large".into()
    }
}

pub fn draw(ui: &mut egui::Ui, app: &App) {
    let p = theme::p();

    egui::ScrollArea::vertical().show(ui, |ui| {
        widgets::section_header(ui, "VRAM Headroom Calculator");

        let vram: Vec<(u64, u64)> = app.gpus.iter().map(|g| (g.vram_used, g.vram_total)).collect();
        let (largest_free, total_free) = aggregate_free(&vram);

        // Show available VRAM per GPU
        for (i, gpu) in app.gpus.iter().enumerate() {
            let free = free_bytes(gpu.vram_used, gpu.vram_total);
            ui.horizontal(|ui| {
                let short = gpu.name.replace("NVIDIA GeForce ", "");
                ui.label(egui::RichText::new(format!("GPU {}: {}", i, short)).size(12.0).color(p.text));
                ui.label(egui::RichText::new(format!(
                    "{} free / {} total",
                    widgets::format_bytes(free),
                    widgets::format_bytes(gpu.vram_total)
                )).size(12.0).color(if free > gpu.vram_total / 4 { p.green } else { p.yellow }));
            });
        }

        if app.gpus.len() > 1 {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Combined free:").size(12.0).color(p.dim));
                ui.label(egui::RichText::new(widgets::format_bytes(total_free)).size(14.0).color(p.accent).strong());
            });
        }

        ui.add_space(12.0);
        widgets::section_header(ui, "What fits?");
        ui.label(egui::RichText::new("Based on current free VRAM (single GPU: largest card)").size(10.0).color(p.dim));
        ui.add_space(4.0);

        let largest_free_gb = bytes_to_gb(largest_free);
        let combined_free_gb = bytes_to_gb(total_free);

        // Model table
        egui_extras::TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::exact(140.0))  // Model
            .column(egui_extras::Column::exact(55.0))   // Q4
            .column(egui_extras::Column::exact(55.0))   // Q5
            .column(egui_extras::Column::exact(55.0))   // Q8
            .column(egui_extras::Column::exact(55.0))   // FP16
            .column(egui_extras::Column::remainder())    // Verdict
            .header(22.0, |mut header| {
                header.col(|ui| { ui.label(egui::RichText::new("Model").size(11.0).color(p.accent).strong()); });
                header.col(|ui| { ui.label(egui::RichText::new("Q4").size(11.0).color(p.accent).strong()); });
                header.col(|ui| { ui.label(egui::RichText::new("Q5").size(11.0).color(p.accent).strong()); });
                header.col(|ui| { ui.label(egui::RichText::new("Q8").size(11.0).color(p.accent).strong()); });
                header.col(|ui| { ui.label(egui::RichText::new("FP16").size(11.0).color(p.accent).strong()); });
                header.col(|ui| { ui.label(egui::RichText::new("Status").size(11.0).color(p.accent).strong()); });
            })
            .body(|body| {
                body.rows(22.0, MODELS.len(), |mut row| {
                    let (name, params, q4, q5, q8, fp16) = MODELS[row.index()];
                    let kv_4k = estimate_kv_cache(params, 4096);

                    row.col(|ui| { ui.label(egui::RichText::new(name).size(11.0)); });

                    for size in [q4, q5, q8, fp16] {
                        row.col(|ui| {
                            let color = if fits(size, kv_4k, largest_free_gb) { p.green } else { p.red };
                            ui.label(egui::RichText::new(format!("{:.0}G", size)).size(11.0).color(color));
                        });
                    }

                    // Verdict
                    row.col(|ui| {
                        let best_fit = verdict(q4, kv_4k, largest_free_gb, combined_free_gb, app.gpus.len());
                        let color = if best_fit.contains("fits") { p.green } else { p.dim };
                        ui.label(egui::RichText::new(best_fit).size(10.0).color(color));
                    });
                });
            });

        ui.add_space(12.0);
        ui.label(egui::RichText::new("Sizes include model weights only. KV cache adds ~0.1-2GB depending on context length. Actual usage varies by runtime.").size(10.0).color(p.dim));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn kv_cache_matches_hand_calculation() {
        // 2 * 32 layers * 8 kv_heads * 128 head_dim * 4096 ctx * 2 bytes
        let expected = (2.0 * 32.0 * 8.0 * 128.0 * 4096.0 * 2.0) / (1024.0 * 1024.0 * 1024.0);
        assert!((kv_cache_gb(4096, 32, 128, 8) - expected).abs() < 1e-12);
        // ~0.5 GiB for an 8B model at 4k context.
        assert!((kv_cache_gb(4096, 32, 128, 8) - 0.5).abs() < 0.01);
    }

    #[test]
    fn kv_cache_scales_linearly_with_context() {
        let a = kv_cache_gb(4096, 80, 128, 8);
        let b = kv_cache_gb(8192, 80, 128, 8);
        assert!((b - 2.0 * a).abs() < 1e-12);
    }

    #[test]
    fn kv_cache_is_zero_for_zero_context() {
        assert_eq!(kv_cache_gb(0, 80, 128, 8), 0.0);
    }

    #[test]
    fn estimate_kv_cache_uses_per_class_shapes() {
        assert_eq!(estimate_kv_cache("8B", 4096), kv_cache_gb(4096, 32, 128, 8));
        assert_eq!(estimate_kv_cache("70B", 4096), kv_cache_gb(4096, 80, 128, 8));
        assert_eq!(estimate_kv_cache("405B", 4096), kv_cache_gb(4096, 126, 128, 8));
        // Unknown class falls back to a fixed guess rather than 0.
        assert_eq!(estimate_kv_cache("3B", 4096), 0.5);
        assert_eq!(estimate_kv_cache("", 4096), 0.5);
        // Bigger model class => bigger cache.
        assert!(estimate_kv_cache("405B", 4096) > estimate_kv_cache("8B", 4096));
    }

    #[test]
    fn every_model_row_has_a_kv_estimate() {
        for (name, params, ..) in MODELS {
            let kv = estimate_kv_cache(params, 4096);
            assert!(kv > 0.0, "{name} ({params}) has no KV estimate");
        }
    }

    #[test]
    fn model_table_is_monotonic_across_quants() {
        for &(name, _, q4, q5, q8, fp16) in MODELS {
            assert!(q4 < q5, "{name}: Q4 should be smaller than Q5");
            assert!(q5 < q8, "{name}: Q5 should be smaller than Q8");
            assert!(q8 < fp16, "{name}: Q8 should be smaller than FP16");
        }
    }

    #[test]
    fn free_bytes_never_underflows() {
        assert_eq!(free_bytes(4 * GB, 24 * GB), 20 * GB);
        // Driver can briefly report used > total; must clamp, not wrap.
        assert_eq!(free_bytes(25 * GB, 24 * GB), 0);
        assert_eq!(free_bytes(0, 0), 0);
    }

    #[test]
    fn dual_gpu_aggregation_5090_plus_4090() {
        // RTX 5090 (32 GiB, 8 used) + RTX 4090 (24 GiB, 4 used).
        let gpus = [(8 * GB, 32 * GB), (4 * GB, 24 * GB)];
        let (largest, combined) = aggregate_free(&gpus);
        assert_eq!(largest, 24 * GB); // 5090 has more free
        assert_eq!(combined, 44 * GB);
    }

    #[test]
    fn largest_is_not_always_the_biggest_card() {
        // 32 GiB card nearly full, 24 GiB card empty.
        let gpus = [(31 * GB, 32 * GB), (0, 24 * GB)];
        let (largest, combined) = aggregate_free(&gpus);
        assert_eq!(largest, 24 * GB);
        assert_eq!(combined, 25 * GB);
    }

    #[test]
    fn aggregation_handles_zero_and_one_gpu() {
        assert_eq!(aggregate_free(&[]), (0, 0));
        assert_eq!(aggregate_free(&[(2 * GB, 8 * GB)]), (6 * GB, 6 * GB));
    }

    #[test]
    fn verdict_single_card_fit() {
        // 70B Q4 = 40 GB, KV ~1.25 GB, single card with 48 GB free.
        let kv = estimate_kv_cache("70B", 4096);
        assert_eq!(verdict(40.0, kv, 48.0, 48.0, 1), "Q4 fits (32k+ctx)");
    }

    #[test]
    fn verdict_split_across_two_cards() {
        // Neither card fits it alone, but combined does.
        let v = verdict(40.0, 1.25, 30.0, 52.0, 2);
        assert_eq!(v, "Q4 fits (split)");
    }

    #[test]
    fn verdict_does_not_claim_split_on_single_gpu() {
        // Same numbers but only one GPU: split is impossible.
        assert_eq!(verdict(40.0, 1.25, 30.0, 52.0, 1), "Too large");
    }

    #[test]
    fn verdict_too_large_for_both() {
        // DeepSeek V3 Q4 = 377 GB.
        assert_eq!(verdict(377.0, 2.0, 30.0, 52.0, 2), "Too large");
    }

    #[test]
    fn verdict_context_hint_shrinks_with_headroom() {
        assert!(verdict(10.0, 0.5, 20.0, 20.0, 1).contains("32k+"));
        assert!(verdict(10.0, 0.5, 13.0, 13.0, 1).contains("8k"));
        assert!(verdict(10.0, 0.5, 10.9, 10.9, 1).contains("4k"));
    }

    #[test]
    fn fits_requires_strict_headroom() {
        // Exactly equal must not count as fitting - no room for overhead.
        assert!(!fits(10.0, 2.0, 12.0));
        assert!(fits(10.0, 2.0, 12.001));
    }

    #[test]
    fn max_context_label_boundaries() {
        assert_eq!(max_context_label(4.1), "32k+");
        assert_eq!(max_context_label(4.0), "8k");
        assert_eq!(max_context_label(1.0), "4k");
        assert_eq!(max_context_label(-3.0), "4k");
    }

    #[test]
    fn bytes_to_gb_uses_gibibytes() {
        assert_eq!(bytes_to_gb(GB), 1.0);
        assert_eq!(bytes_to_gb(0), 0.0);
    }
}
