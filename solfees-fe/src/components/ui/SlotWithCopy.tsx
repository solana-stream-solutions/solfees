import { MouseEventHandler, ReactNode, useRef, useState } from "react";
import { IconCopy } from "@consta/icons/IconCopy";
import { IconCheck } from "@consta/icons/IconCheck";

interface CopyButtonProps {
  value: number;
}

function copyToClipboard(text: string): void {
  const textarea = document.createElement("textarea");
  textarea.textContent = text;
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand("copy");
  document.body.removeChild(textarea);
}
export const SlotWithCopy = ({ value }: CopyButtonProps): ReactNode => {
  const [isTouched, setIsTouched] = useState(false);
  const timeoutId = useRef(0);

  const onCopyHandler: MouseEventHandler<HTMLAnchorElement> = (e) => {
    if (isTouched) return;
    copyToClipboard(`${value}`);
    setIsTouched(true);
    timeoutId.current = +setTimeout(() => setIsTouched(false), 2000);
    const target = e.nativeEvent.target as HTMLElement;
    if (target.nodeName !== "A") e.preventDefault();
  };
  return (
    <a
      href={`https://explorer.solana.com/block/${value}`}
      target="_blank"
      className="hover:underline items-center flex-nowrap flex gap-1"
      onClick={onCopyHandler}
    >
      {value.toLocaleString("en-US")}
      {isTouched ? <IconCheck size="xs" view="success" /> : <IconCopy size="xs" view="secondary" />}
    </a>
  );
};
