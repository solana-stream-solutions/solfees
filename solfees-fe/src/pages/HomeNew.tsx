import type { FunctionComponent } from "../common/types";
import { useState } from "react";
import { useFlag } from "@consta/uikit/useFlag";
import { Epoch } from "../components/ui/Epoch.tsx";
import { useNavigate } from "@tanstack/react-router";
import { IconFeed } from "../components/ui/IconFeed.tsx";
import { ModalFilter } from "../components/ui/ModalFilter.tsx";
import { ModalFee } from "../components/ui/ModalFee.tsx";
import { Footer } from "../components/layout/Footer.tsx";
import { PlotLayer } from "../components/layout/PlotLayer.tsx";
import { CustomTable } from "../components/ui/CustomTable.tsx";

export const HomeNew = (): FunctionComponent => {
  const [filterModalShown, filterModalControls] = useFlag(false);
  const [editedFeeIdx, setEditedFeeIdx] = useState(-1);

  const navigate = useNavigate();

  return (
    <>
      <button
        className="absolute top-2 left-2 rounded bg-amber-300"
        onClick={() => navigate({ to: "/homeOld" })}
      >
        click to view Table from @consta\uikit (old)
      </button>
      <div className="px-20 py-5 bg-white w-full flex-col justify-start items-start gap-8 inline-flex h-[100vh] relative overflow-y-hidden">
        <div className="self-stretch justify-between items-center inline-flex">
          <div className="justify-start items-center gap-2 flex">
            <IconFeed className="w-5 h-5 relative" />
            <div className="text-center text-[#002033] text-2xl font-semibold font-['Inter'] leading-[31.20px]">
              Solfees
            </div>
            <div className="text-center text-[#002033]/60 text-sm font-normal font-['Inter'] leading-[21px]">
              Solana Fees Tracker
            </div>
          </div>
          <Epoch />
        </div>
        <PlotLayer />
        <CustomTable onEditFee={setEditedFeeIdx} onEditKeys={filterModalControls.on} />
        <ModalFee
          editedFeeIdx={editedFeeIdx}
          isVisible={editedFeeIdx >= 0}
          onClose={() => setEditedFeeIdx(-1)}
        />
        <ModalFilter isVisible={filterModalShown} onClose={filterModalControls.off} />
        <Footer />
      </div>
    </>
  );
};
