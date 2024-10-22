import {useEffect, useState} from "react";
import {CommitmentStatus, useWebSocketStore} from "../../store/websocketStore.ts";
import {IconPropView} from "@consta/icons/Icon";
import {IconProcessing} from "@consta/icons/IconProcessing";
import {IconCheck} from "@consta/icons/IconCheck";
import {IconAllDone} from "@consta/icons/IconAllDone";
import {IconWatchStroked} from "@consta/icons/IconWatchStroked";
import {AnimateIconBase} from "@consta/icons/AnimateIconBase";
import {IconPlay} from "@consta/icons/IconPlay";
import {IconPause} from "@consta/icons/IconPause";
import {useFlag} from "@consta/uikit/useFlag";
import {useShallow} from "zustand/react/shallow";


const icons = [IconPause,IconPlay]
export const AnimateIconBaseIcons = () => {
  const suspendQueue = useWebSocketStore(useShallow(state => state.suspendQueue))
  const [isStopped, {toggle}] = useFlag(false);
  useEffect(() => {
    suspendQueue(isStopped)
  }, [isStopped]);

  return (<AnimateIconBase
    onClick={toggle}
    className="shrink-0 cursor-pointer"
    view={isStopped ? 'warning': 'success'}
    icons={icons}
    activeIndex={isStopped ? 0 : 1}
  />);
};
export const Footer = () => {
  return <div className="flex flex-nowrap justify-between w-full">
    <div className="flex flex-nowrap items-center justify-start gap-1">
      <div className="text-[#002033]/60 text-xs font-normal font-['Inter'] leading-none">Powered by</div>
      <a className="" target="_blank" href="https://triton.one/">
        <svg width="77" height="24" viewBox="0 0 77 24" fill="none" xmlns="http://www.w3.org/2000/svg">
          <g filter="url(#filter0_d_5834_12885)">
            <path
              d="M11.9679 19.4033C16.0743 19.4033 19.4033 16.0744 19.4033 11.9679C19.4033 7.86146 16.0743 4.53252 11.9679 4.53252C7.86141 4.53252 4.53247 7.86146 4.53247 11.9679C4.53247 16.0744 7.86141 19.4033 11.9679 19.4033Z"
              fill="url(#paint0_linear_5834_12885)"/>
            <mask id="mask0_5834_12885" style={{maskType: 'alpha'}} maskUnits="userSpaceOnUse" x="4" y="4"
                  width="16"
                  height="16">
              <path
                d="M11.9679 19.4033C16.0743 19.4033 19.4033 16.0744 19.4033 11.9679C19.4033 7.86146 16.0743 4.53252 11.9679 4.53252C7.86141 4.53252 4.53247 7.86146 4.53247 11.9679C4.53247 16.0744 7.86141 19.4033 11.9679 19.4033Z"
                fill="#D9D9D9"/>
            </mask>
            <g mask="url(#mask0_5834_12885)">
              <path
                d="M13.3578 10.7881C8.97358 8.23857 4.75964 9.75458 3.20068 10.8169V22.4449H23.8592V10.4149C22.1855 11.5729 17.742 13.3377 13.3578 10.7881Z"
                fill="url(#paint1_linear_5834_12885)"/>
              <path
                d="M12.1516 11.819C16.8048 9.12087 21.2772 10.7252 22.9318 11.8494V24.1548H1.00586V11.424C2.78227 12.6495 7.4984 14.5171 12.1516 11.819Z"
                fill="url(#paint2_linear_5834_12885)"/>
              <path
                d="M18.1593 16.6541C12.8427 13.5766 7.73256 15.4064 5.84204 16.6888V30.7253H30.8943V16.2036C28.8645 17.6015 23.476 19.7318 18.1593 16.6541Z"
                fill="url(#paint3_linear_5834_12885)"/>
              <path fillRule="evenodd" clipRule="evenodd"
                    d="M11.9679 18.5584C15.6077 18.5584 18.5584 15.6077 18.5584 11.9679C18.5584 8.32805 15.6077 5.37742 11.9679 5.37742C8.32804 5.37742 5.3774 8.32805 5.3774 11.9679C5.3774 15.6077 8.32804 18.5584 11.9679 18.5584ZM11.9679 19.4033C16.0743 19.4033 19.4033 16.0743 19.4033 11.9679C19.4033 7.86143 16.0743 4.53249 11.9679 4.53249C7.86141 4.53249 4.53247 7.86143 4.53247 11.9679C4.53247 16.0743 7.86141 19.4033 11.9679 19.4033Z"
                    fill="white"/>
            </g>
          </g>
          <path d="M26.4081 16.0278V9.64615H24.1014V7.91621H30.7653V9.64615H28.4586V16.0278H26.4081Z" fill="black"/>
          <path
            d="M34.7385 16.0278H32.6881V7.91621H36.5583C37.5749 7.91621 38.3139 8.14685 38.7753 8.6082C39.2452 9.06093 39.4801 9.73159 39.4801 10.62C39.4801 11.2865 39.3135 11.8588 38.9803 12.3373C38.6471 12.8071 38.2071 13.1018 37.6603 13.2215L37.6476 13.2599C37.8611 13.3966 38.0961 13.6229 38.3524 13.9391C38.6087 14.2552 38.8607 14.6011 39.1084 14.977C39.3648 15.3444 39.5912 15.6904 39.7877 16.0151V16.0278H37.4041L36.02 13.9775C35.9004 13.7981 35.8022 13.67 35.7253 13.5931C35.6569 13.5162 35.5715 13.4692 35.469 13.4521C35.375 13.435 35.2212 13.4265 35.0076 13.4265H34.7385V16.0278ZM36.2251 9.5821H34.7385V11.7606H36.2251C36.6523 11.7606 36.947 11.6751 37.1093 11.5043C37.2802 11.3334 37.3656 11.0558 37.3656 10.6713C37.3656 10.2869 37.2802 10.0093 37.1093 9.83837C36.947 9.66755 36.6523 9.5821 36.2251 9.5821Z"
            fill="black"/>
          <path d="M44.0235 16.0278H41.9731V7.91621H44.0235V16.0278Z" fill="black"/>
          <path d="M48.2561 16.0278V9.64615H45.9493V7.91621H52.6132V9.64615H50.3065V16.0278H48.2561Z" fill="black"/>
          <path
            d="M54.2795 11.9656C54.2795 10.9319 54.3949 10.1118 54.6256 9.50521C54.8648 8.89868 55.2706 8.46296 55.843 8.19814C56.4239 7.93325 57.2313 7.80084 58.2651 7.80084C59.0425 7.80084 59.6876 7.87351 60.2002 8.0187C60.7128 8.16396 61.1186 8.39891 61.4176 8.72354C61.7166 9.04817 61.926 9.47958 62.0455 10.0178C62.1737 10.5475 62.2378 11.1967 62.2378 11.9656C62.2378 12.7431 62.1737 13.4009 62.0455 13.9391C61.926 14.4688 61.7166 14.8959 61.4176 15.2206C61.1186 15.5452 60.7128 15.7801 60.2002 15.9253C59.6876 16.0706 59.0425 16.1432 58.2651 16.1432C57.4876 16.1432 56.8426 16.0706 56.33 15.9253C55.8174 15.7801 55.4116 15.5452 55.1126 15.2206C54.8136 14.8959 54.5999 14.4688 54.4718 13.9391C54.3437 13.4009 54.2795 12.7431 54.2795 11.9656ZM58.2651 14.3491C58.8118 14.3491 59.222 14.2808 59.4953 14.1441C59.7687 14.0074 59.9524 13.7682 60.0464 13.4265C60.1404 13.0762 60.1873 12.5893 60.1873 11.9656C60.1873 11.3505 60.1404 10.8721 60.0464 10.5304C59.9524 10.1887 59.7687 9.94947 59.4953 9.81276C59.222 9.66757 58.8118 9.5949 58.2651 9.5949C57.7268 9.5949 57.3168 9.66757 57.0349 9.81276C56.7614 9.94947 56.5735 10.1887 56.471 10.5304C56.377 10.8721 56.33 11.3505 56.33 11.9656C56.33 12.5893 56.377 13.0762 56.471 13.4265C56.5735 13.7682 56.7614 14.0074 57.0349 14.1441C57.3168 14.2808 57.7268 14.3491 58.2651 14.3491Z"
            fill="black"/>
          <path
            d="M69.801 7.91621H71.8514V16.0278H69.8138L67.5071 12.5935C67.3276 12.3202 67.1781 12.0895 67.0585 11.9016C66.9389 11.7136 66.8279 11.5257 66.7253 11.3377C66.6228 11.1497 66.5075 10.9277 66.3793 10.6713L66.3409 10.7226C66.4604 11.1327 66.5331 11.5171 66.5587 11.8759C66.5844 12.2262 66.5972 12.5679 66.5972 12.9011V16.0278H64.5468V7.91621H66.5844L68.891 11.3505C69.0705 11.6153 69.22 11.846 69.3396 12.0425C69.4592 12.2304 69.5703 12.4184 69.6728 12.6063C69.7753 12.7943 69.8907 13.0164 70.0188 13.2727L70.0572 13.2215C69.9718 12.8627 69.9077 12.5166 69.865 12.1835C69.8223 11.8417 69.801 11.5043 69.801 11.1711V7.91621Z"
            fill="black"/>
          <defs>
            <filter id="filter0_d_5834_12885" x="1.26312" y="2.0805" width="21.4094" height="21.4095"
                    filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB">
              <feFlood floodOpacity="0" result="BackgroundImageFix"/>
              <feColorMatrix in="SourceAlpha" type="matrix" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0"
                             result="hardAlpha"/>
              <feOffset dy="0.817337"/>
              <feGaussianBlur stdDeviation="1.63467"/>
              <feComposite in2="hardAlpha" operator="out"/>
              <feColorMatrix type="matrix" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0.2 0"/>
              <feBlend mode="normal" in2="BackgroundImageFix" result="effect1_dropShadow_5834_12885"/>
              <feBlend mode="normal" in="SourceGraphic" in2="effect1_dropShadow_5834_12885" result="shape"/>
            </filter>
            <linearGradient id="paint0_linear_5834_12885" x1="10.0722" y1="6.75496" x2="11.9679" y2="19.4033"
                            gradientUnits="userSpaceOnUse">
              <stop stopColor="#DFDCF2"/>
              <stop offset="0.286458" stopColor="white"/>
            </linearGradient>
            <linearGradient id="paint1_linear_5834_12885" x1="13.5299" y1="9.39079" x2="13.5299" y2="22.4449"
                            gradientUnits="userSpaceOnUse">
              <stop stopColor="#3311FF"/>
              <stop offset="1"/>
            </linearGradient>
            <linearGradient id="paint2_linear_5834_12885" x1="11.9689" y1="10.3402" x2="11.9689" y2="24.1548"
                            gradientUnits="userSpaceOnUse">
              <stop stopColor="#A12CFF"/>
              <stop offset="1"/>
            </linearGradient>
            <linearGradient id="paint3_linear_5834_12885" x1="18.3681" y1="14.9674" x2="18.3681" y2="30.7253"
                            gradientUnits="userSpaceOnUse">
              <stop stopColor="#F606FF"/>
              <stop offset="1"/>
            </linearGradient>
          </defs>
        </svg>
      </a>
    </div>
    <div className="justify-start items-center gap-3 flex">
      <div className="flex flex-nowrap items-center justify-start gap-1">
        <AnimateIconBaseIcons />
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" className="size-6 cursor-pointer active:animate-spin">
          <path fillRule="evenodd"
                d="M7.84 1.804A1 1 0 0 1 8.82 1h2.36a1 1 0 0 1 .98.804l.331 1.652a6.993 6.993 0 0 1 1.929 1.115l1.598-.54a1 1 0 0 1 1.186.447l1.18 2.044a1 1 0 0 1-.205 1.251l-1.267 1.113a7.047 7.047 0 0 1 0 2.228l1.267 1.113a1 1 0 0 1 .206 1.25l-1.18 2.045a1 1 0 0 1-1.187.447l-1.598-.54a6.993 6.993 0 0 1-1.929 1.115l-.33 1.652a1 1 0 0 1-.98.804H8.82a1 1 0 0 1-.98-.804l-.331-1.652a6.993 6.993 0 0 1-1.929-1.115l-1.598.54a1 1 0 0 1-1.186-.447l-1.18-2.044a1 1 0 0 1 .205-1.251l1.267-1.114a7.05 7.05 0 0 1 0-2.227L1.821 7.773a1 1 0 0 1-.206-1.25l1.18-2.045a1 1 0 0 1 1.187-.447l1.598.54A6.992 6.992 0 0 1 7.51 3.456l.33-1.652ZM10 13a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z"
                clipRule="evenodd"/>
        </svg>
        <a className="" target="_blank" href="https://t.me/solfeesio/">
          <svg xmlns="http://www.w3.org/2000/svg" className="size-6" viewBox="0 0 24 24">
            <g>
              <path fill="none" d="M0 0h24v24H0z"/>
              <path
                d="M12 22C6.477 22 2 17.523 2 12S6.477 2 12 2s10 4.477 10 10-4.477 10-10 10zm-3.11-8.83l.013-.007.87 2.87c.112.311.266.367.453.341.188-.025.287-.126.41-.244l1.188-1.148 2.55 1.888c.466.257.801.124.917-.432l1.657-7.822c.183-.728-.137-1.02-.702-.788l-9.733 3.76c-.664.266-.66.638-.12.803l2.497.78z"/>
            </g>
          </svg>
        </a>
        <a className="" target="_blank" href="https://github.com/solana-stream-solutions/solfees">
          <svg className="size-6" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
            <path
              d="M11.9999 2.00002C10.6867 2.00002 9.38636 2.25867 8.1731 2.76122C6.95985 3.26377 5.85746 4.00036 4.92887 4.92895C3.05351 6.80431 1.99994 9.34785 1.99994 12C1.99994 16.42 4.86994 20.17 8.83994 21.5C9.33994 21.58 9.49994 21.27 9.49994 21V19.31C6.72994 19.91 6.13994 17.97 6.13994 17.97C5.67994 16.81 5.02994 16.5 5.02994 16.5C4.11994 15.88 5.09994 15.9 5.09994 15.9C6.09994 15.97 6.62994 16.93 6.62994 16.93C7.49994 18.45 8.96994 18 9.53994 17.76C9.62994 17.11 9.88994 16.67 10.1699 16.42C7.94994 16.17 5.61994 15.31 5.61994 11.5C5.61994 10.39 5.99994 9.50002 6.64994 8.79002C6.54994 8.54002 6.19994 7.50002 6.74994 6.15002C6.74994 6.15002 7.58994 5.88002 9.49994 7.17002C10.2899 6.95002 11.1499 6.84002 11.9999 6.84002C12.8499 6.84002 13.7099 6.95002 14.4999 7.17002C16.4099 5.88002 17.2499 6.15002 17.2499 6.15002C17.7999 7.50002 17.4499 8.54002 17.3499 8.79002C17.9999 9.50002 18.3799 10.39 18.3799 11.5C18.3799 15.32 16.0399 16.16 13.8099 16.41C14.1699 16.72 14.4999 17.33 14.4999 18.26V21C14.4999 21.27 14.6599 21.59 15.1699 21.5C19.1399 20.16 21.9999 16.42 21.9999 12C21.9999 10.6868 21.7413 9.38644 21.2387 8.17318C20.7362 6.95992 19.9996 5.85753 19.071 4.92895C18.1424 4.00036 17.04 3.26377 15.8268 2.76122C14.6135 2.25867 13.3132 2.00002 11.9999 2.00002Z"
              fill="#002033"/>
          </svg>
        </a>
        <a className="" target="_blank"
           href="https://github.com/solana-stream-solutions/solfees?tab=readme-ov-file#donations">
          <svg className="size-6" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
            <path
              d="M11.9997 21.35L10.5497 20.03C5.39973 15.36 1.99973 12.27 1.99973 8.49998C1.99973 5.40998 4.41973 2.99998 7.49973 2.99998C9.23973 2.99998 10.9097 3.80998 11.9997 5.07998C13.0897 3.80998 14.7597 2.99998 16.4997 2.99998C19.5797 2.99998 21.9997 5.40998 21.9997 8.49998C21.9997 12.27 18.5997 15.36 13.4497 20.03L11.9997 21.35Z"
              fill="#002033"/>
          </svg>
        </a>
      </div>
      <a className="text-[#002033]/60 text-xs font-normal font-['Inter'] leading-none"
         href="https://github.com/solana-stream-solutions/" target="_blank">Developed by Solana Stream Solutions</a>
    </div>
  </div>
}