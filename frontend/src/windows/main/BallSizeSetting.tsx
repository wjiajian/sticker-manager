import { BallFace } from "../../components/BallFace";
import { ControlSkeleton, SettingRow } from "./PreferencesLayout";
import { BALL_SIZE, type useBallSize } from "./useBallSize";

export function BallSizeSetting({ id, ball, loading, visible, disabled }: {
  id: string; ball: ReturnType<typeof useBallSize>; loading: boolean; visible: boolean; disabled: boolean;
}) {
  return <SettingRow item="ball-size" stacked dim={!visible} label={<label htmlFor={id}>悬浮球大小</label>}>
    <div className={`flex items-center gap-3 ${visible || loading ? "" : "opacity-50"}`}>
      {loading ? <><ControlSkeleton shape="preview" /><ControlSkeleton shape="slider" /></> : <>
        <div aria-hidden className="grid size-24 shrink-0 place-items-center rounded-card bg-muted"><BallFace size={ball.displayBallSize} /></div>
        <input type="range" id={id} {...ball.inputProps} disabled={disabled || !visible}
          className="tile-slider interactive min-w-0 flex-1 hover:opacity-82" />
        <span className="w-11 text-right text-xs text-ink-secondary">{ball.displayBallSize} px</span>
        {ball.displayBallSize !== BALL_SIZE.default && <button type="button" disabled={disabled || !visible} onClick={ball.resetToDefault}
          className="text-xs text-ink-secondary underline underline-offset-4 hover:text-ink">恢复默认</button>}
      </>}
    </div>
  </SettingRow>;
}
