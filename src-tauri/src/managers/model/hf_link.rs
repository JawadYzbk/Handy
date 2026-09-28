use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct HfModelFile {
    pub filename: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct HfRepoInfo {
    pub repo_id: String,
    pub revision: String,
    pub model_name: String,
    pub description: Option<String>,
    pub selected_file: Option<String>,
    pub available_files: Vec<HfModelFile>,
}

#[derive(Debug, Deserialize)]
struct HfSibling {
    rfilename: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HfModelApiResponse {
    id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    siblings: Vec<HfSibling>,
}

#[derive(Debug, Deserialize)]
struct HfTreeItem {
    #[serde(rename = "type")]
    item_type: String,
    path: String,
    #[serde(default)]
    size: Option<u64>,
}

/// Parses any format of Hugging Face URL or repository specifier into
/// (repo_id, revision, optional_direct_filename).
pub fn parse_hf_url(input: &str) -> Result<(String, String, Option<String>), String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("URL or repository ID cannot be empty".to_string());
    }

    // Strip scheme
    let without_scheme = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);

    // Strip domain if present
    let path = if let Some(rest) = without_scheme.strip_prefix("huggingface.co/") {
        rest
    } else if let Some(rest) = without_scheme.strip_prefix("hf.co/") {
        rest
    } else if let Some(rest) = without_scheme.strip_prefix("www.huggingface.co/") {
        rest
    } else {
        without_scheme
    };

    // Strip query parameters and anchors
    let clean_path = path
        .split(&['?', '#'][..])
        .next()
        .unwrap_or(path)
        .trim_matches('/');

    let parts: Vec<&str> = clean_path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return Err(
            "Please provide a valid Hugging Face repository in the format 'owner/repo' or a direct model link."
                .to_string(),
        );
    }

    let owner = parts[0];
    let repo_name = parts[1];
    let repo_id = format!("{}/{}", owner, repo_name);

    if parts.len() == 2 {
        return Ok((repo_id, "main".to_string(), None));
    }

    // Check for /blob/, /resolve/, /raw/, /tree/
    let marker = parts[2];
    if (marker == "blob" || marker == "resolve" || marker == "raw") && parts.len() >= 5 {
        let revision = parts[3].to_string();
        let filename = parts[4..].join("/");
        return Ok((repo_id, revision, Some(filename)));
    } else if marker == "tree" && parts.len() >= 4 {
        let revision = parts[3].to_string();
        let filename = if parts.len() > 4 {
            Some(parts[4..].join("/"))
        } else {
            None
        };
        return Ok((repo_id, revision, filename));
    } else if parts.len() == 3 {
        let file = parts[2];
        if is_compatible_model_filename(file) {
            return Ok((repo_id, "main".to_string(), Some(file.to_string())));
        }
    }

    Ok((repo_id, "main".to_string(), None))
}

/// Checks whether a filename is a supported model format for transcribe-cpp / Handy.
pub fn is_compatible_model_filename(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".gguf") || lower.ends_with(".bin")
}

fn quant_rank(filename: &str) -> u8 {
    let lower = filename.to_lowercase();
    if lower.contains("q5_k_m") || lower.contains("q5_0") || lower.contains("q5_1") {
        1
    } else if lower.contains("q4_k_m") || lower.contains("q4_0") || lower.contains("q4_1") {
        2
    } else if lower.contains("q6_k") {
        3
    } else if lower.contains("q8_0") {
        4
    } else if lower.contains("q3_k") {
        5
    } else if lower.contains("q2_k") {
        6
    } else if lower.contains("bf16") || lower.contains("f16") {
        7
    } else if lower.contains("f32") {
        8
    } else {
        10
    }
}

/// Queries the Hugging Face API to inspect model repository details and available files.
pub async fn inspect_hf_url(url: &str) -> Result<HfRepoInfo, String> {
    let (repo_id, revision, direct_filename) = parse_hf_url(url)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Handy-Desktop")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // 1. Fetch model metadata
    let api_url = format!("https://huggingface.co/api/models/{}", repo_id);
    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| format!("Network error querying Hugging Face: {}", e))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("Repository '{}' was not found on Hugging Face.", repo_id));
    } else if resp.status() == reqwest::StatusCode::UNAUTHORIZED
        || resp.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(format!(
            "Repository '{}' is private or requires authentication.",
            repo_id
        ));
    } else if !resp.status().is_success() {
        return Err(format!(
            "Hugging Face API returned error status: {}",
            resp.status()
        ));
    }

    let model_data: HfModelApiResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Hugging Face API response: {}", e))?;

    // 2. Fetch tree listing with file sizes if available
    let tree_url = format!("https://huggingface.co/api/models/{}/tree/{}", repo_id, revision);
    let mut file_sizes: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    if let Ok(tree_resp) = client.get(&tree_url).send().await {
        if tree_resp.status().is_success() {
            if let Ok(items) = tree_resp.json::<Vec<HfTreeItem>>().await {
                for item in items {
                    if item.item_type == "file" {
                        if let Some(sz) = item.size {
                            file_sizes.insert(item.path, sz);
                        }
                    }
                }
            }
        }
    }

    // 3. Build available model files list
    let mut available_files: Vec<HfModelFile> = Vec::new();
    let mut seen_filenames = std::collections::HashSet::new();

    // First collect from siblings or tree
    for sibling in &model_data.siblings {
        if is_compatible_model_filename(&sibling.rfilename) {
            let sz = file_sizes.get(&sibling.rfilename).copied();
            seen_filenames.insert(sibling.rfilename.clone());
            available_files.push(HfModelFile {
                filename: sibling.rfilename.clone(),
                size_bytes: sz,
            });
        }
    }

    // If tree had files not present in siblings
    for (path, &sz) in &file_sizes {
        if is_compatible_model_filename(path) && !seen_filenames.contains(path) {
            available_files.push(HfModelFile {
                filename: path.clone(),
                size_bytes: Some(sz),
            });
        }
    }

    // If a direct filename was requested in the URL and isn't in list yet, include it
    if let Some(ref direct) = direct_filename {
        if !available_files.iter().any(|f| &f.filename == direct) {
            let sz = file_sizes.get(direct).copied();
            available_files.push(HfModelFile {
                filename: direct.clone(),
                size_bytes: sz,
            });
        }
    }

    // Sort available files (optimal quants first: Q5_K_M, Q4_K_M, Q8_0, then others)
    available_files.sort_by(|a, b| {
        let rank_a = quant_rank(&a.filename);
        let rank_b = quant_rank(&b.filename);
        rank_a.cmp(&rank_b).then_with(|| a.filename.cmp(&b.filename))
    });

    // 4. Determine selected file: direct requested file, or best quant
    let selected_file = if let Some(ref direct) = direct_filename {
        Some(direct.clone())
    } else if let Some(best) = available_files.iter().find(|f| f.filename.contains("Q5_K_M") || f.filename.contains("q5_k_m")) {
        Some(best.filename.clone())
    } else if let Some(best) = available_files.iter().find(|f| f.filename.contains("Q4_K_M") || f.filename.contains("q4_k_m")) {
        Some(best.filename.clone())
    } else if let Some(first) = available_files.first() {
        Some(first.filename.clone())
    } else {
        None
    };

    let model_name = repo_id
        .split('/')
        .nth(1)
        .unwrap_or(&repo_id)
        .replace(['-', '_'], " ");

    Ok(HfRepoInfo {
        repo_id,
        revision,
        model_name,
        description: model_data.description,
        selected_file,
        available_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hf_url_full_blob() {
        let (repo, rev, file) = parse_hf_url(
            "https://huggingface.co/openai/whisper-large-v3-turbo/blob/main/model.gguf",
        )
        .unwrap();
        assert_eq!(repo, "openai/whisper-large-v3-turbo");
        assert_eq!(rev, "main");
        assert_eq!(file, Some("model.gguf".to_string()));
    }

    #[test]
    fn test_parse_hf_url_resolve_subpath() {
        let (repo, rev, file) = parse_hf_url(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/v1.5/models/ggml-base.bin?download=true",
        )
        .unwrap();
        assert_eq!(repo, "ggerganov/whisper.cpp");
        assert_eq!(rev, "v1.5");
        assert_eq!(file, Some("models/ggml-base.bin".to_string()));
    }

    #[test]
    fn test_parse_hf_url_repo_only() {
        let (repo, rev, file) =
            parse_hf_url("https://huggingface.co/openai/whisper-large-v3-turbo").unwrap();
        assert_eq!(repo, "openai/whisper-large-v3-turbo");
        assert_eq!(rev, "main");
        assert_eq!(file, None);
    }

    #[test]
    fn test_parse_hf_url_short_repo_and_file() {
        let (repo, rev, file) = parse_hf_url("ggerganov/whisper.cpp/ggml-base.en.bin").unwrap();
        assert_eq!(repo, "ggerganov/whisper.cpp");
        assert_eq!(rev, "main");
        assert_eq!(file, Some("ggml-base.en.bin".to_string()));
    }

    #[test]
    fn test_parse_hf_url_hf_co_domain() {
        let (repo, rev, file) =
            parse_hf_url("https://hf.co/Systran/faster-whisper-large-v3/blob/main/model.bin")
                .unwrap();
        assert_eq!(repo, "Systran/faster-whisper-large-v3");
        assert_eq!(rev, "main");
        assert_eq!(file, Some("model.bin".to_string()));
    }
}
