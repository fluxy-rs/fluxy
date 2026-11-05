import { useNavigate, useParams } from "@solidjs/router";
import { ChatWorkspace } from "../components/ChatWorkspace";

/// The message view. The open conversation is part of the URL, so the phone's
/// system back button returns to the list instead of leaving the chat section.
export function ChatPage() {
  const params = useParams();
  const navigate = useNavigate();
  return (
    <div class="flex min-h-0 flex-1 flex-col">
      <ChatWorkspace
        selectedId={params.deviceId}
        onSelect={(deviceId) => navigate(deviceId ? `/chat/${deviceId}` : "/chat")}
      />
    </div>
  );
}
