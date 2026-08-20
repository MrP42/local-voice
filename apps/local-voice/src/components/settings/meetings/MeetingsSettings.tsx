import React, { useState } from "react";
import type { Meeting } from "@/bindings";
import { RecorderCard } from "./RecorderCard";
import { LiveTranscript } from "./LiveTranscript";
import { MeetingList } from "./MeetingList";
import { MeetingDetail } from "./MeetingDetail";
import { MeetingLanguageSetting } from "./MeetingLanguageSetting";
import { MeetingModelSetting } from "./MeetingModelSetting";
import { MeetingRetentionSetting } from "./MeetingRetentionSetting";
import { SettingsGroup } from "../../ui/SettingsGroup";

export const MeetingsSettings: React.FC = () => {
  const [selected, setSelected] = useState<Meeting | null>(null);

  if (selected) {
    return (
      <div className="w-full space-y-6">
        <MeetingDetail
          meeting={selected}
          onBack={() => setSelected(null)}
          onMeetingChange={setSelected}
        />
      </div>
    );
  }

  return (
    <div className="w-full space-y-6">
      <RecorderCard />
      <LiveTranscript />
      <MeetingList onSelect={setSelected} />
      <SettingsGroup>
        <MeetingLanguageSetting />
        <MeetingModelSetting />
        <MeetingRetentionSetting />
      </SettingsGroup>
    </div>
  );
};
