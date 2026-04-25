interface Props {
  svg: string;
  width: number;
  height: number;
  title?: string;
}

export default function Flag({ svg, width, height, title }: Props) {
  if (!svg) return null;
  const wrapperStyle: React.CSSProperties = {
    display: 'inline-flex',
    width,
    height,
    border: '1px solid rgba(0,0,0,0.4)',
    boxShadow: '0 1px 2px rgba(0,0,0,0.4)',
    flexShrink: 0,
    overflow: 'hidden',
  };
  return (
    <span
      style={wrapperStyle}
      title={title}
      aria-label={title}
      dangerouslySetInnerHTML={{
        __html: svg.replace(
          /^<svg /,
          `<svg style="width:100%;height:100%;display:block" preserveAspectRatio="xMidYMid meet" `,
        ),
      }}
    />
  );
}
