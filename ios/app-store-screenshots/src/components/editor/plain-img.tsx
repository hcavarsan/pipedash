import Image, { type ImageProps } from 'next/image'
import type { ReactNode } from 'react'

type PlainImgProps = Omit<ImageProps, 'src' | 'width' | 'height' | 'unoptimized' | 'loading' | 'fill'> & {
  src: string
}

export function PlainImg(props: PlainImgProps): ReactNode {
  return <Image {...props} width={0} height={0} sizes="100vw" unoptimized loading="eager" />
}
