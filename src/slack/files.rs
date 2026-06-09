use anyhow::{anyhow, Result};
use std::io::Write;

use super::SlackClient;

impl SlackClient {
    #[allow(dead_code)]
    pub async fn download_file(&self, file_id: &str, _channel_id: &str) -> Result<std::path::PathBuf> {
        tracing::debug!("=== DOWNLOAD FILE DEBUG ===");
        tracing::debug!("file_id: {}", file_id);
        
        // First, get file info to get the download URL
        let file_info_url = format!("https://slack.com/api/files.info?file={}", file_id);
        tracing::debug!("Requesting file info from: {}", file_info_url);
        
        let file_info_response: serde_json::Value = self
            .http
            .get(&file_info_url)
            .bearer_auth(&self.token)
            .send()
            .await?
            .json()
            .await?;

        tracing::debug!("File info response: {}", serde_json::to_string_pretty(&file_info_response).unwrap_or_default());

        if !file_info_response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error = file_info_response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
            tracing::debug!("Failed to get file info: {}", error);
            return Err(anyhow!("Failed to get file info: {}", error));
        }

        let file = file_info_response.get("file").ok_or_else(|| {
            tracing::debug!("No file data in response");
            anyhow!("No file data")
        })?;
        
        tracing::debug!("File data: {}", serde_json::to_string_pretty(file).unwrap_or_default());
        
        let url_private = file.get("url_private")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                tracing::debug!("No url_private in file data");
                anyhow!("No download URL")
            })?;
        
        tracing::debug!("Download URL: {}", url_private);
        
        let file_name = file.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("file");
        
        tracing::debug!("File name: {}", file_name);
        
        // Create store directory if it doesn't exist
        let store_dir = std::path::Path::new("store");
        tracing::debug!("Creating store directory: {:?}", store_dir);
        std::fs::create_dir_all(store_dir)?;
        
        // Download the file
        tracing::debug!("Starting file download...");
        let response = self
            .http
            .get(url_private)
            .bearer_auth(&self.token)
            .send()
            .await?;
        
        tracing::debug!("Download response status: {}", response.status());
        
        if !response.status().is_success() {
            tracing::debug!("Download failed with status: {}", response.status());
            return Err(anyhow!("Failed to download file: {}", response.status()));
        }
        
        let file_path = store_dir.join(file_name);
        tracing::debug!("Saving file to: {:?}", file_path);
        
        let mut file = std::fs::File::create(&file_path)?;
        let bytes = response.bytes().await?;
        tracing::debug!("Received {} bytes", bytes.len());
        
        file.write_all(&bytes)?;
        tracing::debug!("File saved successfully to: {:?}", file_path);
        
        Ok(file_path)
    }

    /// Extract redirect URL from HTML response (handles meta refresh, window.location, etc.)
    fn extract_redirect_from_html(html: &str) -> Option<String> {
        tracing::debug!("=== EXTRACT REDIRECT FROM HTML ===");
        
        // First, try to find URL in JSON data (data-props, entryPoint, etc.)
        // Look for "entryPoint":"https:\/\/files.slack.com...
        if let Some(entry_start) = html.find("\"entryPoint\"") {
            tracing::debug!("Found entryPoint in JSON data");
            let after_entry = &html[entry_start..];
            // Look for the URL after entryPoint
            if let Some(url_start_pos) = after_entry.find("https:\\/\\/files.slack.com") {
                let url_part = &after_entry[url_start_pos..];
                // Find the end of the URL (until quote or comma)
                let mut url_end = url_part.len();
                for (i, c) in url_part.char_indices() {
                    if c == '"' || c == ',' || c == '}' {
                        url_end = i;
                        break;
                    }
                }
                // Also check for HTML entities like &quot; at the end
                if let Some(amp_pos) = url_part[..url_end].rfind('&') {
                    if url_part[amp_pos..].starts_with("&quot;") || url_part[amp_pos..].starts_with("&amp;") {
                        url_end = amp_pos;
                    }
                }
                let escaped_url = &url_part[..url_end];
                // Unescape the URL
                let mut url = escaped_url.replace("\\/", "/").replace("\\\"", "\"").replace("\\'", "'");
                // Remove any trailing HTML entities or quotes
                url = url.trim_end_matches("&quot;").trim_end_matches("&amp;").trim_end_matches('"').trim_end_matches('\'').to_string();
                tracing::debug!("Found URL in entryPoint: {}", url);
                if url.starts_with("https://files.slack.com") && !url.contains("/beacon/") && !url.contains("/tracking/") {
                    return Some(url);
                }
            }
        }
        
        // Also look for escaped https://files.slack.com directly
        if let Some(start) = html.find("https:\\/\\/files.slack.com") {
            tracing::debug!("Found escaped https://files.slack.com");
            let url_part = &html[start..];
            let mut url_end = url_part.len();
            for (i, c) in url_part.char_indices() {
                if c == '"' || c == '\'' || c == ' ' || c == '>' || c == '<' || 
                   c == ')' || c == ';' || c == ',' || c == '}' || c == ']' || c == '\n' || c == '\r' {
                    url_end = i;
                    break;
                }
            }
            // Also check for HTML entities like &quot; at the end
            if url_end < url_part.len() && url_part[url_end..].starts_with("&quot;") {
                // Already stopped before &quot;
            } else if let Some(amp_pos) = url_part[..url_end].rfind('&') {
                // Check if there's an HTML entity at the end
                if url_part[amp_pos..].starts_with("&quot;") || url_part[amp_pos..].starts_with("&amp;") {
                    url_end = amp_pos;
                }
            }
            let escaped_url = &url_part[..url_end];
            let mut url = escaped_url.replace("\\/", "/").replace("\\\"", "\"").replace("\\'", "'");
            // Remove any trailing HTML entities or quotes
            url = url.trim_end_matches("&quot;").trim_end_matches("&amp;").trim_end_matches('"').trim_end_matches('\'').to_string();
            tracing::debug!("Found escaped URL: {}", url);
            if url.starts_with("https://files.slack.com") && !url.contains("/beacon/") && !url.contains("/tracking/") {
                return Some(url);
            }
        }
        
        // Then, find ALL occurrences of files.slack.com and extract the full URLs
        let mut search_start = 0;
        while let Some(start) = html[search_start..].find("files.slack.com") {
            let absolute_start = search_start + start;
            
            // Find the start of the URL (go backwards to find https:// or http://)
            let mut url_start = absolute_start;
            let mut found_protocol = false;
            // Look backwards up to 200 characters to find the protocol
            let max_lookback = absolute_start.min(200);
            for i in (0..max_lookback).rev() {
                let check_start = absolute_start.saturating_sub(i);
                if check_start + 7 <= html.len() && &html[check_start..check_start + 7] == "http://" {
                    url_start = check_start;
                    found_protocol = true;
                    break;
                }
                if check_start + 8 <= html.len() && &html[check_start..check_start + 8] == "https://" {
                    url_start = check_start;
                    found_protocol = true;
                    break;
                }
            }
            
            if found_protocol {
                tracing::debug!("Found protocol at position {}", url_start);
                // Find the end of the URL (until quote, space, or other delimiter)
                let url_part = &html[url_start..];
                let mut url_end = url_part.len();
                for (i, c) in url_part.char_indices() {
                    if c == '"' || c == '\'' || c == ' ' || c == '>' || c == '<' || 
                       c == ')' || c == ';' || c == ',' || c == '}' || c == ']' || c == '\n' || c == '\r' {
                        url_end = i;
                        break;
                    }
                }
                let url = url_part[..url_end].to_string();
                tracing::debug!("Found potential URL: {}", url);
                
                // Filter out tracking URLs - accept any files.slack.com URL that's not tracking
                if !url.contains("/beacon/") && !url.contains("/tracking/") && 
                   !url.contains("/analytics/") && !url.contains("/api/") {
                    // Unescape the URL if needed
                    let unescaped_url = url.replace("\\/", "/").replace("\\\"", "\"").replace("\\'", "'");
                    tracing::debug!("Unescaped URL: {}", unescaped_url);
                    // Make sure it's a valid URL
                    if unescaped_url.starts_with("http://") || unescaped_url.starts_with("https://") {
                        tracing::debug!("Returning valid URL: {}", unescaped_url);
                        return Some(unescaped_url);
                    } else {
                        tracing::debug!("URL doesn't start with http:// or https://");
                    }
                } else {
                    tracing::debug!("URL filtered out (contains tracking/beacon/analytics/api)");
                }
            } else {
                tracing::debug!("Could not find protocol before files.slack.com at position {}", absolute_start);
            }
            
            // Move search forward
            search_start = absolute_start + 1;
            if search_start >= html.len() {
                break;
            }
        }
        
        // Fallback: Look for direct download link (href to files.slack.com)
        if let Some(start) = html.find("href=\"https://files.slack.com") {
            let url_part = &html[start + 6..];
            if let Some(url_end) = url_part.find('"') {
                let url = url_part[..url_end].to_string();
                // Filter out tracking URLs
                if !url.contains("/beacon/") && !url.contains("/tracking/") {
                    return Some(url);
                }
            }
        }
        
        // Look for files.slack.com in any URL pattern (simple version)
        if let Some(start) = html.find("https://files.slack.com") {
            // Find the full URL (until quote, space, or end of string)
            let url_part = &html[start..];
            let mut url_end = url_part.len();
            for (i, c) in url_part.char_indices() {
                if c == '"' || c == '\'' || c == ' ' || c == '>' || c == '<' || c == ')' || c == ';' {
                    url_end = i;
                    break;
                }
            }
            let url = url_part[..url_end].to_string();
            // Filter out tracking URLs
            if !url.contains("/beacon/") && !url.contains("/tracking/") {
                return Some(url);
            }
        }
        
        // Look for meta refresh redirect (but filter out tracking URLs)
        if let Some(start) = html.find("http-equiv=\"refresh\"") {
            if let Some(content_start) = html[start..].find("content=\"") {
                let content = &html[start + content_start + 9..];
                if let Some(url_start) = content.find("url=") {
                    let url_part = &content[url_start + 4..];
                    if let Some(url_end) = url_part.find('"') {
                        let url = url_part[..url_end].to_string();
                        // Filter out tracking URLs
                        if !url.contains("/beacon/") && !url.contains("/tracking/") && url.contains("files.slack.com") {
                            return Some(url);
                        }
                    }
                }
            }
        }
        
        // Look for window.location redirect (but filter out tracking URLs)
        if let Some(start) = html.find("window.location") {
            let after_location = &html[start..];
            if let Some(url_start) = after_location.find("= \"") {
                let url_part = &after_location[url_start + 3..];
                if let Some(url_end) = url_part.find('"') {
                    let url = url_part[..url_end].to_string();
                    // Filter out tracking URLs and prioritize files.slack.com
                    if !url.contains("/beacon/") && !url.contains("/tracking/") && url.contains("files.slack.com") {
                        return Some(url);
                    }
                }
            }
            if let Some(url_start) = after_location.find("= '") {
                let url_part = &after_location[url_start + 3..];
                if let Some(url_end) = url_part.find('\'') {
                    let url = url_part[..url_end].to_string();
                    // Filter out tracking URLs and prioritize files.slack.com
                    if !url.contains("/beacon/") && !url.contains("/tracking/") && url.contains("files.slack.com") {
                        return Some(url);
                    }
                }
            }
        }
        
        tracing::debug!("No valid files.slack.com URL found in HTML");
        None
    }

    pub async fn download_file_from_url(&self, url: &str, file_name: &str) -> Result<std::path::PathBuf> {
        use std::collections::HashSet;
        
        let mut redirect_count = 0;
        let mut current_url = url.to_string();
        let mut tried_urls = HashSet::new();
        
        loop {
            if redirect_count > 5 {
                return Err(anyhow!("Too many redirects (max 5)"));
            }
            
            // Check if we've already tried this URL (avoid infinite loops)
            if tried_urls.contains(&current_url) {
                tracing::debug!("URL redirect loop detected: already tried {}", current_url);
                return Err(anyhow!("URL redirect loop detected. The file URL requires authentication that we cannot provide. Try adding 'files:write:user' scope to your Slack app for direct file downloads."));
            }
            tried_urls.insert(current_url.clone());
            
            tracing::debug!("=== DOWNLOAD FILE FROM URL DEBUG (redirect {}) ===", redirect_count);
            tracing::debug!("URL: {}", current_url);
            tracing::debug!("File name: {}", file_name);
            
            // Create store directory if it doesn't exist
            let store_dir = std::path::Path::new("store");
            if redirect_count == 0 {
                tracing::debug!("Creating store directory: {:?}", store_dir);
                std::fs::create_dir_all(store_dir)?;
            }
            
            // Download the file directly from URL
            tracing::debug!("Starting file download from URL...");
            let request = self
                .http
                .get(&current_url)
                .bearer_auth(&self.token)
                .header("Accept", "*/*")
                .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36");
            
            // If this is a redirect, try to preserve cookies from previous request
            // (reqwest Client should handle this automatically, but we can be explicit)
            let response = request.send().await?;
        
        tracing::debug!("Download response status: {}", response.status());
        
        // Log response headers
        let headers = response.headers();
        tracing::debug!("Response headers:");
        for (name, value) in headers.iter() {
            if let Ok(value_str) = value.to_str() {
                tracing::debug!("  {}: {}", name, value_str);
            } else {
                tracing::debug!("  {}: <binary>", name);
            }
        }
        
        // Check content-type - if it's HTML, something went wrong
        let content_type = headers.get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        tracing::debug!("Content-Type: {}", content_type);
        
            if content_type.contains("text/html") {
                tracing::debug!("WARNING: Received HTML instead of file. Attempting to extract redirect URL from HTML...");
                
                // Read the HTML response
                let html_bytes = response.bytes().await?;
                let html = String::from_utf8_lossy(&html_bytes);
                tracing::debug!("HTML response (first 1000 chars): {}", &html.chars().take(1000).collect::<String>());
                
                // Also log if we can find any files.slack.com URLs in the HTML
                let mut search_pos = 0;
                let mut occurrence_count = 0;
                while let Some(pos) = html[search_pos..].find("files.slack.com") {
                    let absolute_pos = search_pos + pos;
                    occurrence_count += 1;
                    let start = absolute_pos.saturating_sub(100);
                    let end = (absolute_pos + 200).min(html.len());
                    let context = &html[start..end];
                    tracing::debug!("Context around files.slack.com #{}: ...{}...", occurrence_count, context);
                    search_pos = absolute_pos + 1;
                    if search_pos >= html.len() {
                        break;
                    }
                }
                tracing::debug!("Found {} mentions of 'files.slack.com' in HTML", occurrence_count);
                
                // Also try to find the URL in a different way - look for the file ID pattern
                if let Some(file_id_pos) = html.find("F0ACD4WMTV2") {
                    let start = file_id_pos.saturating_sub(50);
                    let end = (file_id_pos + 150).min(html.len());
                    let context = &html[start..end];
                    tracing::debug!("Context around file ID: ...{}...", context);
                }
                
                // Try to find a redirect URL in the HTML (common patterns)
                // Look for meta refresh, window.location, or direct download links
                if let Some(redirect_url) = Self::extract_redirect_from_html(&html) {
                    tracing::debug!("Found redirect URL in HTML: {}", redirect_url);
                    // Update URL and continue loop
                    current_url = redirect_url;
                    redirect_count += 1;
                    continue;
                }
                
                tracing::debug!("ERROR: Could not extract redirect URL from HTML.");
                return Err(anyhow!("Received HTML response instead of file, and could not find redirect URL."));
            }
            
            if !response.status().is_success() {
                tracing::debug!("Download failed with status: {}", response.status());
                return Err(anyhow!("Failed to download file: {}", response.status()));
            }
            
            // Sanitize file name to avoid issues with special characters
            let sanitized_name = file_name
                .chars()
                .map(|c| if c.is_control() || c == '/' || c == '\\' { '_' } else { c })
                .collect::<String>();
            
            let file_path = store_dir.join(&sanitized_name);
            tracing::debug!("Saving file to: {:?} (sanitized from: {})", file_path, file_name);
            
            // Read all bytes and write to file
            let bytes = response.bytes().await?;
            tracing::debug!("Received {} bytes", bytes.len());
            
            // Check first few bytes to verify it's valid
            if bytes.len() >= 8 {
                let header = &bytes[0..8.min(bytes.len())];
                tracing::debug!("File header (first {} bytes): {:?}", header.len(), header);
                
                // Verify it's not HTML
                if header.starts_with(b"<!DOCTYPE") || header.starts_with(b"<html") {
                    tracing::debug!("ERROR: File appears to be HTML, not a binary file!");
                    return Err(anyhow!("Downloaded file appears to be HTML, not the actual file."));
                }
            }
            
            let mut file = std::fs::File::create(&file_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?; // Ensure all data is written to disk
            tracing::debug!("File saved successfully to: {:?}", file_path);
            
            return Ok(file_path);
        }
    }

    pub async fn get_shared_public_url(&self, file_id: &str, file_name: &str) -> Result<std::path::PathBuf> {
        tracing::debug!("=== GET SHARED PUBLIC URL DEBUG ===");
        tracing::debug!("file_id: {}, file_name: {}", file_id, file_name);
        
        // Use files.sharedPublicURL API to get a direct download URL
        let share_url = format!("https://slack.com/api/files.sharedPublicURL?file={}", file_id);
        tracing::debug!("Requesting shared public URL from: {}", share_url);
        
        let share_response: serde_json::Value = self
            .http
            .get(&share_url)
            .bearer_auth(&self.token)
            .send()
            .await?
            .json()
            .await?;

        tracing::debug!("Share response: {}", serde_json::to_string_pretty(&share_response).unwrap_or_default());

        if !share_response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error = share_response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
            let needed = share_response.get("needed").and_then(|v| v.as_str()).unwrap_or("");
            tracing::debug!("Failed to get shared public URL: {} (needed: {})", error, needed);
            if error == "missing_scope" {
                return Err(anyhow!("Missing scope '{}'. Please add this scope to your Slack app's OAuth scopes and reinstall the app.", needed));
            }
            return Err(anyhow!("Failed to get shared public URL: {}", error));
        }

        // Get the download URL from the share response
        let file = share_response.get("file").ok_or_else(|| {
            tracing::debug!("No file data in share response");
            anyhow!("No file data in share response")
        })?;
        
        // Try permalink_public first (public share URL), then url_private_download
        let download_url = file.get("permalink_public")
            .or_else(|| file.get("url_private_download"))
            .or_else(|| file.get("url_private"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                tracing::debug!("No download URL in share response");
                anyhow!("No download URL in share response")
            })?;
        
        tracing::debug!("Got download URL from share: {}", download_url);
        
        // Now download the file
        self.download_file_from_url(download_url, file_name).await
    }

    #[allow(dead_code)]
    pub async fn download_file_by_id(&self, file_id: &str, file_name: &str) -> Result<std::path::PathBuf> {
        tracing::debug!("=== DOWNLOAD FILE BY ID DEBUG ===");
        tracing::debug!("file_id: {}, file_name: {}", file_id, file_name);
        
        // Get file info to get url_private_download
        let file_info_url = format!("https://slack.com/api/files.info?file={}", file_id);
        tracing::debug!("Requesting file info from: {}", file_info_url);
        
        let file_info_response: serde_json::Value = self
            .http
            .get(&file_info_url)
            .bearer_auth(&self.token)
            .send()
            .await?
            .json()
            .await?;

        tracing::debug!("File info response: {}", serde_json::to_string_pretty(&file_info_response).unwrap_or_default());

        if !file_info_response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error = file_info_response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
            tracing::debug!("Failed to get file info: {}", error);
            return Err(anyhow!("Failed to get file info: {}", error));
        }

        let file = file_info_response.get("file").ok_or_else(|| {
            tracing::debug!("No file data in response");
            anyhow!("No file data")
        })?;
        
        // Prefer url_private_download, fallback to url_private
        let download_url = file.get("url_private_download")
            .or_else(|| file.get("url_private"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                tracing::debug!("No download URL in file data");
                anyhow!("No download URL")
            })?;
        
        tracing::debug!("Got download URL: {}", download_url);
        
        // Now download the file
        self.download_file_from_url(download_url, file_name).await
    }
}
