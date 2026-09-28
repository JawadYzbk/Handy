import React, { useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  Download,
  Search,
  Loader2,
  FileBox,
  AlertCircle,
  ExternalLink,
} from "lucide-react";
import { Dialog } from "@/components/ui/Dialog";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Select, type SelectOption } from "@/components/ui/Select";
import { useModelStore } from "@/stores/modelStore";
import type { HfRepoInfo } from "@/bindings";

interface HuggingFaceDownloadModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function formatBytes(bytes?: number | null): string {
  if (!bytes || bytes <= 0) return "";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(2)} GB`;
  }
  return `${Math.round(mb)} MB`;
}

export const HuggingFaceDownloadModal: React.FC<
  HuggingFaceDownloadModalProps
> = ({ open, onOpenChange }) => {
  const { t } = useTranslation();
  const { inspectHuggingFaceUrl, downloadHuggingFaceModel } = useModelStore();

  const [url, setUrl] = useState("");
  const [inspecting, setInspecting] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [repoInfo, setRepoInfo] = useState<HfRepoInfo | null>(null);
  const [selectedFilename, setSelectedFilename] = useState<string | null>(null);

  const inputRef = useRef<HTMLInputElement>(null);

  const handleInspect = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const trimmed = url.trim();
    if (!trimmed) {
      setError(t("settings.models.huggingface.invalidUrl"));
      return;
    }

    setInspecting(true);
    setError(null);
    setRepoInfo(null);
    setSelectedFilename(null);

    try {
      const info = await inspectHuggingFaceUrl(trimmed);
      setRepoInfo(info);
      if (info.selected_file) {
        setSelectedFilename(info.selected_file);
      } else if (info.available_files.length > 0) {
        setSelectedFilename(info.available_files[0].filename);
      }
    } catch (err) {
      console.error("Failed to inspect Hugging Face URL:", err);
      setError(
        typeof err === "string"
          ? err
          : err instanceof Error
            ? err.message
            : t("settings.models.huggingface.error"),
      );
    } finally {
      setInspecting(false);
    }
  };

  const handleDownload = async () => {
    if (!repoInfo || !selectedFilename) return;

    setDownloading(true);
    try {
      const success = await downloadHuggingFaceModel(
        repoInfo.repo_id,
        repoInfo.revision || "main",
        selectedFilename,
      );

      if (success) {
        toast.success(
          t("settings.models.huggingface.downloadStarted", {
            filename: selectedFilename,
          }),
        );
        handleClose();
      }
    } catch (err) {
      console.error("Failed to start download:", err);
      toast.error(
        typeof err === "string"
          ? err
          : err instanceof Error
            ? err.message
            : t("settings.models.huggingface.downloadFailed"),
      );
    } finally {
      setDownloading(false);
    }
  };

  const handleClose = () => {
    onOpenChange(false);
    setUrl("");
    setError(null);
    setRepoInfo(null);
    setSelectedFilename(null);
    setInspecting(false);
    setDownloading(false);
  };

  const fileOptions: SelectOption[] =
    repoInfo?.available_files.map((file) => {
      const formattedSize = formatBytes(file.size_bytes);
      return {
        value: file.filename,
        label: formattedSize
          ? `${file.filename} (${formattedSize})`
          : file.filename,
      };
    }) || [];

  const selectedFile = repoInfo?.available_files.find(
    (f) => f.filename === selectedFilename,
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(isOpen) => (!isOpen ? handleClose() : onOpenChange(isOpen))}
      title={t("settings.models.huggingface.modalTitle")}
      description={t("settings.models.huggingface.modalDescription")}
      closeLabel={t("common.close")}
      className="max-w-lg w-full"
      initialFocusRef={inputRef as React.RefObject<HTMLElement>}
    >
      <div className="space-y-4">
        {/* Input & Inspect */}
        <form onSubmit={handleInspect} className="space-y-2">
          <label className="block text-xs font-medium text-text/70">
            {t("settings.models.huggingface.inputLabel")}
          </label>
          <div className="flex gap-2">
            <Input
              ref={inputRef}
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder={t("settings.models.huggingface.inputPlaceholder")}
              className="flex-1"
              disabled={inspecting || downloading}
            />
            <Button
              type="submit"
              disabled={!url.trim() || inspecting || downloading}
              variant="primary"
              size="md"
              className="flex items-center gap-1.5 shrink-0"
            >
              {inspecting ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  <span>{t("settings.models.huggingface.inspecting")}</span>
                </>
              ) : (
                <>
                  <Search className="w-4 h-4" />
                  <span>{t("settings.models.huggingface.inspect")}</span>
                </>
              )}
            </Button>
          </div>
          <p className="text-[11px] text-text/50">
            {t("settings.models.huggingface.hint")}
          </p>
        </form>

        {/* Error Alert */}
        {error && (
          <div className="flex items-start gap-2 p-3 text-xs bg-red-500/10 border border-red-500/20 text-red-500 rounded-lg">
            <AlertCircle className="w-4 h-4 shrink-0 mt-0.5" />
            <div className="flex-1">{error}</div>
          </div>
        )}

        {/* Inspected Repo Details */}
        {repoInfo && (
          <div className="space-y-3 p-3 bg-mid-gray/5 border border-mid-gray/30 rounded-lg">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <FileBox className="w-4 h-4 text-logo-primary" />
                <span className="text-sm font-semibold text-text">
                  {repoInfo.model_name}
                </span>
              </div>
              <a
                href={`https://huggingface.co/${repoInfo.repo_id}`}
                target="_blank"
                rel="noreferrer"
                className="text-xs text-logo-primary hover:underline flex items-center gap-1"
              >
                <span>{repoInfo.repo_id}</span>
                <ExternalLink className="w-3 h-3" />
              </a>
            </div>

            {repoInfo.description && (
              <p className="text-xs text-text/60 line-clamp-2">
                {repoInfo.description}
              </p>
            )}

            {repoInfo.available_files.length > 0 ? (
              <div className="space-y-2 pt-2 border-t border-mid-gray/20">
                <label className="block text-xs font-medium text-text/70">
                  {t("settings.models.huggingface.selectFile")}
                </label>
                <Select
                  value={selectedFilename}
                  options={fileOptions}
                  onChange={(val) => setSelectedFilename(val)}
                  disabled={downloading}
                  placeholder={t("settings.models.huggingface.chooseFile")}
                />
              </div>
            ) : (
              <div className="text-xs text-text/50 py-2 text-center border-t border-mid-gray/20">
                {t("settings.models.huggingface.noFilesFound")}
              </div>
            )}
          </div>
        )}

        {/* Footer Actions */}
        <div className="flex justify-end gap-2 pt-2">
          <Button
            type="button"
            onClick={handleClose}
            variant="secondary"
            disabled={downloading}
          >
            {t("common.cancel")}
          </Button>
          {repoInfo && repoInfo.available_files.length > 0 && (
            <Button
              type="button"
              onClick={handleDownload}
              disabled={!selectedFilename || downloading}
              variant="primary"
              className="flex items-center gap-1.5"
            >
              {downloading ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  <span>{t("settings.models.huggingface.downloading")}</span>
                </>
              ) : (
                <>
                  <Download className="w-4 h-4" />
                  <span>
                    {t("settings.models.huggingface.download")}
                    {selectedFile?.size_bytes
                      ? ` (${formatBytes(selectedFile.size_bytes)})`
                      : ""}
                  </span>
                </>
              )}
            </Button>
          )}
        </div>
      </div>
    </Dialog>
  );
};
