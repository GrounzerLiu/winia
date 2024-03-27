use crate::hct::Cam;
use crate::palettes::TonalPalette;

#[derive(Clone, Copy, Default)]
pub struct CorePalette{
    primary: TonalPalette,
    secondary: TonalPalette,
    tertiary: TonalPalette,
    neutral: TonalPalette,
    neutral_variant: TonalPalette,
    error: TonalPalette,
}

impl CorePalette{
    fn new(hue: f64, chroma: f64, is_content:bool)->Self{
        Self{
            primary:TonalPalette::from_hue_and_chroma(hue, primary_chroma(chroma, is_content)),
            secondary:TonalPalette::from_hue_and_chroma(hue, secondary_chroma(chroma, is_content)),
            tertiary:TonalPalette::from_hue_and_chroma(hue+60.0, tertiary_chroma(chroma, is_content)),
            neutral:TonalPalette::from_hue_and_chroma(hue, neutral_chroma(chroma, is_content)),
            neutral_variant:TonalPalette::from_hue_and_chroma(hue, neutral_variant_chroma(chroma, is_content)),
            error:TonalPalette::from_hue_and_chroma(25.0, 84.0),
        }
    }
    pub fn from_hue_and_chroma(hue: f64, chroma: f64)->Self{
        Self::new(hue, chroma, false)
    }

    pub fn content_from_hue_and_chroma(hue: f64, chroma: f64)->Self{
        Self::new(hue, chroma, true)
    }

    pub fn from_argb(argb: u32)->Self{
        let cam = Cam::from_argb(argb);
        Self::new(cam.hue, cam.chroma, false)
    }

    pub fn content_from_argb(argb: u32)->Self{
        let cam = Cam::from_argb(argb);
        Self::new(cam.hue, cam.chroma, true)
    }

    pub fn primary(&self)->TonalPalette{
        self.primary
    }

    pub fn secondary(&self)->TonalPalette{
        self.secondary
    }

    pub fn tertiary(&self)->TonalPalette{
        self.tertiary
    }

    pub fn neutral(&self)->TonalPalette{
        self.neutral
    }

    pub fn neutral_variant(&self)->TonalPalette{
        self.neutral_variant
    }

    pub fn error(&self)->TonalPalette{
        self.error
    }
}


fn primary_chroma(chroma: f64, is_content: bool)->f64{
    if is_content{
        chroma
    }else{
        chroma.max(48.0)
    }
}

fn secondary_chroma(chroma: f64, is_content: bool)->f64{
    if is_content{
        chroma / 3.0
    }else{
        16.0
    }
}

fn tertiary_chroma(chroma: f64, is_content: bool)->f64{
    if is_content{
        chroma / 2.0
    }else{
        24.0
    }
}

fn neutral_chroma(chroma: f64, is_content: bool)->f64{
    if is_content{
        (chroma / 12.0).min(4.0)
    }else{
        4.0
    }
}

fn neutral_variant_chroma(chroma: f64, is_content: bool)->f64{
    if is_content{
        (chroma / 6.0).min(8.0)
    }else{
        8.0
    }
}