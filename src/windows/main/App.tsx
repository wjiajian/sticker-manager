import { DetailPanel } from "./DetailPanel";
import { Sidebar } from "./Sidebar";
import { StickerGrid } from "./StickerGrid";
import { TitleBar } from "./TitleBar";

export function App() {
  return (
    <div className="flex h-full flex-col">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <StickerGrid />
        <DetailPanel />
      </div>
    </div>
  );
}
