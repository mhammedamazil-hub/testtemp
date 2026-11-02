import type { ApiError } from "../lib/api";
import type { AppConfig, Job, UploadImage } from "../lib/types";
import type { GenerationSettings } from "./SettingsCard";
import { PreprocessCard } from "./PreprocessCard";
import { SettingsCard } from "./SettingsCard";
import { UploadCard } from "./UploadCard";

interface StudioPanelProps {
  config: AppConfig | null;
  upload: UploadImage | null;
  uploading: boolean;
  uploadError: ApiError | null;
  preprocessJob: Job | null;
  preprocessBusy: boolean;
  preprocessError: ApiError | null;
  settings: GenerationSettings;
  onSettingsChange: (patch: Partial<GenerationSettings>) => void;
  onFile: (file: File) => void;
  onSample: (url: string) => void;
  onClear: () => void;
  onGenerate: () => void;
  canGenerate: boolean;
  simulated: boolean;
  generating: boolean;
  blockedReason: string | null;
}

export function StudioPanel(props: StudioPanelProps) {
  return (
    <div className="space-y-4">
      <UploadCard
        upload={props.upload}
        uploading={props.uploading}
        error={props.uploadError}
        demoImage={props.config?.demoImage}
        maxUploadMb={props.config?.maxUploadMb ?? 20}
        onFile={props.onFile}
        onSample={props.onSample}
        onClear={props.onClear}
      />
      {props.upload && (
        <PreprocessCard
          job={props.preprocessJob}
          busy={props.preprocessBusy}
          error={props.preprocessError}
          enabled={props.config?.features.preprocess ?? true}
        />
      )}
      <SettingsCard
        config={props.config}
        settings={props.settings}
        onChange={props.onSettingsChange}
        onGenerate={props.onGenerate}
        canGenerate={props.canGenerate}
        simulated={props.simulated}
        generating={props.generating}
        blockedReason={props.blockedReason}
      />
    </div>
  );
}
