export type Coin = {
  service: 'PCGS' | 'NGC' | 'ANACS'
  // string, not number: NGC certs look like 1234567-001 and leading zeros matter
  cert: string
  name: string
  grade: string
  // obverse first, pasted from the PCGS/NGC cert page; may be empty.
  // ANACS publishes no photos: use your own from public/coins/, e.g. '/coins/anacs-1234567.jpg'
  images: string[]
}

// ponytail: hand-pasted and hotlinked. If PCGS/NGC block or move images, copy them into public/coins/ instead.
export const coins: Coin[] = [
  // {
  //   service: 'PCGS',
  //   cert: '12345678',
  //   name: '1881-S Morgan Dollar',
  //   grade: 'MS 65',
  //   images: ['https://...'],
  // },
  {
    service: 'PCGS',
    cert: '88547873',
    name: '2024 10 Yn Panda Silver Panda 2024 flower, FDoI (L#372)',
    grade: 'MS 70',
    images: ['https://d1htnxwo4o0jhw.cloudfront.net/cert/149832462/small/4WQH_svQlEqKtCGVPxuIMA.png'],
  },
  {
    service: 'PCGS',
    cert: '44935416',
    name: '(1934)二十三 $1 LM-110 K-624',
    grade: 'MS 63',
    images: ['https://d1htnxwo4o0jhw.cloudfront.net/cert/126667291/small/265058505.jpg'],
  },
  {
    service: 'PCGS',
    cert: '49206003',
    name: '(1914)三 $1 Y-329 LM-63 圎 NC',
    grade: 'AU 50',
    images: ['https://d1htnxwo4o0jhw.cloudfront.net/cert/154803644/small/CNvxfa8SO0WgM_Tn5kb5Gw.jpg'],
  },
  {
    service: 'PCGS',
    cert: '49206005',
    name: 'Y-329.6 LM-79 "T" 年 CN',
    grade: 'AU 58',
    images: ['https://d1htnxwo4o0jhw.cloudfront.net/cert/154803646/small/mbchgDOUI0Cp0jrr7hF8Qg.jpg'],
  },
  {
    service: 'NGC',
    cert: '6428608-035',
    name: 'S39//1964 JAPAN TOKYO OLYMPICS 1000Y',
    grade: 'MS 66',
    images: [
      'https://ccg-imaging-ngc-coins-production.s3.amazonaws.com/7e76b746-aa6c-47cb-97d2-4330de1ae324/NGC6428608-035_OBV@500x702.jpg',
      'https://ccg-imaging-ngc-coins-production.s3.amazonaws.com/7e76b746-aa6c-47cb-97d2-4330de1ae324/NGC6428608-035_REV@500x702.jpg'
    ],
  },
  {
    service: 'NGC',
    cert: '6461439-037',
    name: '1941 MALAYA 10C',
    grade: 'MS 64',
    images: [
      'https://ccg-imaging-ngc-coins-production.s3.amazonaws.com/2bc7cdf6-a1f1-44d0-b646-cdf1c17ab862/NGC6461439-037_OBV@500x688.jpg',
      'https://ccg-imaging-ngc-coins-production.s3.amazonaws.com/2bc7cdf6-a1f1-44d0-b646-cdf1c17ab862/NGC6461439-037_REV@500x688.jpg'
    ],
  },
  {
    service: 'ANACS',
    cert: '4626405',
    name: '1919 STRAITS CENT',
    grade: 'EF 40',
    images: [
      'https://ccg-imaging-ngc-coins-production.s3.amazonaws.com/ef54165459c0105fd1be239049159fdad2345caa/NGC4502486-001_OBV%40200X400.JPG',
      'https://ccg-imaging-ngc-coins-production.s3.amazonaws.com/ef54165459c0105fd1be239049159fdad2345caa/NGC4502486-001_REV%40200X400.JPG'
    ],
  },
  {
    service: 'NGC',
    cert: '4502486-001',
    name: '1920 STRAITS CENT',
    grade: 'MS 65 RB',
    images: [
      'https://ccg-imaging-ngc-coins-production.s3.amazonaws.com/ef54165459c0105fd1be239049159fdad2345caa/NGC4502486-001_OBV%40200X400.JPG',
      'https://ccg-imaging-ngc-coins-production.s3.amazonaws.com/ef54165459c0105fd1be239049159fdad2345caa/NGC4502486-001_REV%40200X400.JPG'
    ],
  },
  {
    service: 'PCGS',
    cert: '43565862',
    name: '1927 10C',
    grade: 'MS 64',
    images: [
      'https://d1htnxwo4o0jhw.cloudfront.net/cert/126180506/small/259256415.jpg',
    ],
  },
]

export function certUrl({ service, cert, grade }: Coin): string {
  switch (service) {
    case 'PCGS':
      return `https://www.pcgs.com/cert/${cert}`
    case 'NGC':
      // NGC lookup needs the numeric grade as well as the cert number
      return `https://www.ngccoin.com/certlookup/${cert}/${grade.replace(/\D/g, '')}/`
    case 'ANACS':
      // ponytail: ANACS form only, no known per-cert link; visitor types the cert shown on the card.
      return 'https://portal.anacs.com/Verify/CertVerification.aspx'
  }
}
