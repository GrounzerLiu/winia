use std::hash::Hash;
use crate::hct::cam::Cam;
use crate::hct::hct_solver::solve_to_int;
use crate::utils::{Argb, lstar_from_argb};

#[derive(Debug,Clone,Copy,Default)]
pub struct Hct{
    hue: f64,
    chroma: f64,
    tone: f64,
    argb: Argb
}

impl Hash for Hct{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.argb.hash(state);
    }
}

impl PartialEq for Hct{
    fn eq(&self,other: &Self)->bool{
        self.hue==other.hue && self.chroma==other.chroma && self.tone==other.tone&&self.argb==other.argb
    }
}

impl Eq for Hct{}

impl Hct{
    pub fn from_hct(hue: f64, chroma: f64, tone: f64)->Hct{
        let argb=solve_to_int(hue,chroma,tone);
        let cam = Cam::from_argb(argb);
        Hct{
            hue: cam.hue,
            chroma: cam.chroma,
            tone: lstar_from_argb(argb),
            argb
        }
    }

    pub fn from_argb(argb: Argb)->Hct{
        let cam = Cam::from_argb(argb);
        Hct{
            hue: cam.hue,
            chroma: cam.chroma,
            tone: lstar_from_argb(argb),
            argb
        }
    }

    pub fn hue(&self)->f64{
        self.hue
    }

    pub fn chroma(&self)->f64{
        self.chroma
    }

    pub fn tone(&self)->f64{
        self.tone
    }

    pub fn to_argb(&self) ->Argb{
        self.argb
    }

    pub fn set_hue(&mut self,hue: f64){
        self.set_internal_state(solve_to_int(hue,self.chroma,self.tone))
    }

    pub fn set_chroma(&mut self,chroma: f64){
        self.set_internal_state(solve_to_int(self.hue,chroma,self.tone))
    }

    pub fn set_tone(&mut self,tone: f64){
        self.set_internal_state(solve_to_int(self.hue,self.chroma,tone))
    }

    fn set_internal_state(&mut self,argb:Argb){
        self.argb=argb;
        let cam = Cam::from_argb(argb);
        self.hue=cam.hue;
        self.chroma=cam.chroma;
        self.tone=lstar_from_argb(argb);
    }
}

