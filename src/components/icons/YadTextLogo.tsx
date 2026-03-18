/* eslint-disable i18next/no-literal-string */
import React from "react";

const YadTextLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox="0 0 240 100"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Stroke/outline layer */}
      <text
        x="120"
        y="78"
        textAnchor="middle"
        fontFamily="'SF Pro Rounded', 'Nunito', 'Quicksand', 'Varela Round', system-ui, sans-serif"
        fontWeight="900"
        fontSize="88"
        strokeLinejoin="round"
        strokeLinecap="round"
        className="logo-stroke"
        strokeWidth="12"
        paintOrder="stroke"
        letterSpacing="-2"
      >
        Yad
      </text>
      {/* Fill layer */}
      <text
        x="120"
        y="78"
        textAnchor="middle"
        fontFamily="'SF Pro Rounded', 'Nunito', 'Quicksand', 'Varela Round', system-ui, sans-serif"
        fontWeight="900"
        fontSize="88"
        className="logo-primary"
        letterSpacing="-2"
      >
        Yad
      </text>
    </svg>
  );
};

export default YadTextLogo;
