import React from "react";
import { RecorderCard } from "./RecorderCard";
import { LiveTranscript } from "./LiveTranscript";

export const MeetingsSettings: React.FC = () => (
  <div className="max-w-3xl w-full mx-auto space-y-6">
    <RecorderCard />
    <LiveTranscript />
    {/* Task 14 ergänzt: <MeetingList /> */}
  </div>
);
