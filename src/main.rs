use std::{env, fs::File, io::{BufWriter, Write}, sync::Arc, thread};
use minifb::{Key, MouseButton, MouseMode, Scale, Window, WindowOptions};

const W: usize = 480;
const H: usize = 360;
const INTERACTIVE_W: usize = 120;
const INTERACTIVE_H: usize = 90;
const MAX_DEPTH: u32 = 4;

#[derive(Clone, Copy, Debug, Default)]
struct V { x:f64, y:f64, z:f64 }
impl V {
    fn new(x:f64,y:f64,z:f64)->Self{Self{x,y,z}}
    fn dot(self,b:Self)->f64{self.x*b.x+self.y*b.y+self.z*b.z}
    fn cross(self,b:Self)->Self{Self::new(self.y*b.z-self.z*b.y,self.z*b.x-self.x*b.z,self.x*b.y-self.y*b.x)}
    fn len(self)->f64{self.dot(self).sqrt()}
    fn norm(self)->Self{self/self.len().max(1e-12)}
    fn reflect(self,n:Self)->Self{self-n*(2.0*self.dot(n))}
    fn refract(self,n:Self,ior:f64)->Option<Self>{let mut c=(-self).dot(n).clamp(-1.0,1.0);let mut nn=n;let mut eta=1.0/ior;if c<0.0{c=-c}else{nn=-n;eta=ior}let k=1.0-eta*eta*(1.0-c*c);if k<0.0{None}else{Some(self*eta+nn*(eta*c-k.sqrt()))}}
    fn had(self,b:Self)->Self{Self::new(self.x*b.x,self.y*b.y,self.z*b.z)}
    fn max(self)->f64{self.x.max(self.y).max(self.z)}
    fn clamp(self)->Self{Self::new(self.x.clamp(0.0,1.0),self.y.clamp(0.0,1.0),self.z.clamp(0.0,1.0))}
}
use std::ops::{Add,Sub,Mul,Div,Neg};
impl Add for V{type Output=Self;fn add(self,b:Self)->Self{Self::new(self.x+b.x,self.y+b.y,self.z+b.z)}}
impl Sub for V{type Output=Self;fn sub(self,b:Self)->Self{Self::new(self.x-b.x,self.y-b.y,self.z-b.z)}}
impl Mul<f64> for V{type Output=Self;fn mul(self,s:f64)->Self{Self::new(self.x*s,self.y*s,self.z*s)}}
impl Div<f64> for V{type Output=Self;fn div(self,s:f64)->Self{self*(1.0/s)}}
impl Neg for V{type Output=Self;fn neg(self)->Self{self*(-1.0)}}

#[derive(Clone,Copy)]
struct Ray{ o:V,d:V }
#[derive(Clone,Copy)]
struct Mat{ albedo:V, spec:f64, trans:f64, refl:f64, ior:f64, emission:V, tex:u8, normal_map:bool }
const GRASS:Mat=Mat{albedo:V{x:0.20,y:0.55,z:0.23},spec:0.08,trans:0.0,refl:0.02,ior:1.0,emission:V{x:0.0,y:0.0,z:0.0},tex:1,normal_map:true};
const SOIL:Mat=Mat{albedo:V{x:0.31,y:0.18,z:0.12},spec:0.04,trans:0.0,refl:0.0,ior:1.0,emission:V{x:0.0,y:0.0,z:0.0},tex:2,normal_map:false};
const ROCK:Mat=Mat{albedo:V{x:0.37,y:0.43,z:0.49},spec:0.52,trans:0.0,refl:0.18,ior:1.0,emission:V{x:0.0,y:0.0,z:0.0},tex:3,normal_map:true};
const WATER:Mat=Mat{albedo:V{x:0.10,y:0.48,z:0.58},spec:0.92,trans:0.20,refl:0.36,ior:1.33,emission:V{x:0.0,y:0.0,z:0.0},tex:4,normal_map:true};
const GLASS:Mat=Mat{albedo:V{x:0.72,y:0.88,z:0.96},spec:0.96,trans:0.88,refl:0.08,ior:1.50,emission:V{x:0.0,y:0.0,z:0.0},tex:5,normal_map:false};
const WOOD:Mat=Mat{albedo:V{x:0.42,y:0.23,z:0.09},spec:0.18,trans:0.0,refl:0.02,ior:1.0,emission:V{x:0.0,y:0.0,z:0.0},tex:6,normal_map:false};
const LAMP:Mat=Mat{albedo:V{x:0.95,y:0.52,z:0.10},spec:0.1,trans:0.0,refl:0.0,ior:1.0,emission:V{x:5.0,y:2.3,z:0.55},tex:7,normal_map:false};

#[derive(Clone,Copy)] struct Hit{t:f64,p:V,n:V,mat:Mat}
#[derive(Clone,Copy)] struct Box3{lo:V,hi:V,mat:Mat}
impl Box3{
 fn hit(self,r:Ray,tmin:f64,tmax:f64)->Option<Hit>{let mut near=tmin;let mut far=tmax;let mut nn=V::default();for a in 0..3{let (o,d,l,h)=match a{0=>(r.o.x,r.d.x,self.lo.x,self.hi.x),1=>(r.o.y,r.d.y,self.lo.y,self.hi.y),_=>(r.o.z,r.d.z,self.lo.z,self.hi.z)};if d.abs()<1e-10{if o<l||o>h{return None}continue}let mut t0=(l-o)/d;let mut t1=(h-o)/d;let mut n=match a{0=>V::new(-1.0,0.0,0.0),1=>V::new(0.0,-1.0,0.0),_=>V::new(0.0,0.0,-1.0)};if t0>t1{std::mem::swap(&mut t0,&mut t1);n=-n}if t0>near{near=t0;nn=n}far=far.min(t1);if far<=near{return None}}if near>tmin{Some(Hit{t:near,p:r.o+r.d*near,n:nn,mat:self.mat})}else{None}}
}
struct Scene{boxes:Vec<Box3>, spin:f64, day:bool }
fn hash(x:i32,y:i32,z:i32,seed:u64)->u32{let mut n=(x as u32).wrapping_mul(374761393).wrapping_add((y as u32).wrapping_mul(668265263)).wrapping_add((z as u32).wrapping_mul(1442695041)).wrapping_add(seed as u32).wrapping_add((seed>>32) as u32);n=(n^(n>>13)).wrapping_mul(1274126177);n^(n>>16)}
fn noise(x:f64,z:f64,seed:u64)->f64{let ix=x.floor() as i32;let iz=z.floor() as i32;let fx=x-ix as f64;let fz=z-iz as f64;let smooth=|t:f64|t*t*(3.0-2.0*t);let a=hash(ix,0,iz,seed) as f64/u32::MAX as f64;let b=hash(ix+1,0,iz,seed) as f64/u32::MAX as f64;let c=hash(ix,0,iz+1,seed) as f64/u32::MAX as f64;let d=hash(ix+1,0,iz+1,seed) as f64/u32::MAX as f64;let u=smooth(fx);let v=smooth(fz);(a*(1.0-u)+b*u)*(1.0-v)+(c*(1.0-u)+d*u)*v}
fn terrain_height(x:i32,z:i32,seed:u64)->i32{let r=((x*x+z*z) as f64).sqrt();if r>11.5{return 0}let n=noise(x as f64*0.42,z as f64*0.42,seed);let ridge=(1.0-r/12.0).max(0.0);(1+((n-0.42)*6.0).round() as i32+(ridge*2.0) as i32).clamp(1,5)}
fn add_box(v:&mut Vec<Box3>,x:f64,y:f64,z:f64,sx:f64,sy:f64,sz:f64,m:Mat){v.push(Box3{lo:V::new(x,y,z),hi:V::new(x+sx,y+sy,z+sz),mat:m})}
fn build_scene(spin:f64,day:bool,seed:u64)->Scene{let mut b=Vec::with_capacity(1000);for x in -12..12{for z in -12..12{let h=terrain_height(x,z,seed);if h==0{continue}for y in 0..h{let m=if y==h-1{GRASS}else{SOIL};add_box(&mut b,x as f64,y as f64,z as f64,1.0,1.0,1.0,m)}}}
 // stepped mountain / exposed rock face
 for k in 0..5{let y=5.0+k as f64;let r=2.6-k as f64*0.42;for x in -5..=5{for z in -5..=5{if (x*x+z*z) as f64<=r*r&&terrain_height(x,z,seed)>=5{add_box(&mut b,x as f64,y,z as f64,1.0,1.0,1.0,ROCK)}}}}
 // winding stream, stones and a small wooden crossing
 for i in -9..=8{let z=(i as f64*0.45).round() as i32;let x=i;let h=terrain_height(x,z,seed);if h>1{add_box(&mut b,x as f64,h as f64+0.02,z as f64,1.0,0.12,1.0,WATER)}if i%3==0{add_box(&mut b,x as f64,h as f64+0.12,z as f64,0.55,0.5,0.55,ROCK)}}
 for x in -2..=2{let top=terrain_height(x,2,seed).max(terrain_height(x+1,2,seed)).max(terrain_height(x,3,seed)).max(terrain_height(x+1,3,seed)) as f64;add_box(&mut b,x as f64,top+0.05,2.0,0.9,0.35,0.8,WOOD)}
 // glass observatory, wooden supports, emissive crystal and four lanterns
 for x in [-2.0,1.0]{for z in [-3.0,0.0]{let h=terrain_height(x as i32,z as i32,seed) as f64;add_box(&mut b,x,h,z,0.28,3.0,0.28,WOOD)}}
 for x in [-2.0,1.0]{let h=terrain_height(x as i32,-3,seed) as f64+2.0;add_box(&mut b,x,h,-3.0,3.0,0.12,0.12,WOOD)}
 for (x,z) in [(-2.0,-3.0),(-2.0,-1.0),(0.7,-3.0),(0.7,-1.0)]{let h=terrain_height(x as i32,z as i32,seed) as f64+1.0;add_box(&mut b,x,h,z,0.42,1.5,0.42,GLASS)}
 add_box(&mut b,-0.55,terrain_height(0,-2,seed) as f64+1.7,-2.0,0.85,1.1,0.85,LAMP);
 for (x,z) in [(-8.0,-4.0),(7.0,-5.0),(-6.0,6.0),(6.0,5.0)]{let h=terrain_height(x as i32,z as i32,seed) as f64;add_box(&mut b,x,h,z,0.16,2.3,0.16,WOOD);add_box(&mut b,x-0.23,h+1.85,z-0.23,0.62,0.55,0.62,LAMP)}
 // second, larger glass lens by the water
 let lens_y=terrain_height(5,2,seed) as f64;add_box(&mut b,5.5,lens_y,2.0,0.8,1.8,0.8,GLASS);
 Scene{boxes:b,spin,day}}
fn texture(mat:Mat,p:V,n:V)->V{let scale=match mat.tex{1=>1.0,2=>3.0,3=>2.0,4=>1.7,5=>1.0,6=>4.0,_=>1.0};let ix=(p.x*scale).floor() as i32;let iy=(p.y*scale).floor() as i32;let iz=(p.z*scale).floor() as i32;let h=hash(ix,iy,iz,0);let checker=((ix^iy^iz)&1) as f64;let mut c=mat.albedo;match mat.tex{1=>{c=c*(0.84+0.16*(h as f64/u32::MAX as f64));},2=>c=c*(0.68+0.30*(h as f64/u32::MAX as f64)),3=>c=c*(0.65+0.35*checker),4=>{c=c*(0.8+0.2*((p.x*3.0+p.z*4.0).sin()*0.5+0.5));},5=>{},6=>c=c*(0.72+0.25*((p.x*9.0).sin()*0.5+0.5)),_=>{}}
 if mat.normal_map{let q=V::new((p.x*7.0).sin()*0.10,(p.z*7.0).cos()*0.10,(p.x*5.0+p.z*4.0).sin()*0.10);c=c*(0.94+0.06*n.dot((n+q).norm()).abs())}c}
fn sky(d:V,day:bool)->V{let t=0.5*(d.y+1.0);if day{let mut c=V::new(0.38,0.66,0.88)*(1.0-t)+V::new(0.10,0.38,0.78)*t;let sun=V::new(-0.4,0.58,-0.7).norm();let disc=d.dot(sun).max(0.0).powf(180.0);c=c+V::new(1.0,0.82,0.48)*disc*2.5;c}else{let mut c=V::new(0.025,0.045,0.11)*(1.0-t)+V::new(0.15,0.31,0.53)*t;let stars=hash((d.x*900.0) as i32,(d.y*900.0) as i32,(d.z*900.0) as i32,0);if d.y>0.05&&stars%430==0{c=c+V::new(0.8,0.9,1.0)*(0.7+(stars%100) as f64/100.0)}let sun=V::new(-0.4,0.58,-0.7).norm();let disc=d.dot(sun).max(0.0).powf(380.0);c+V::new(0.9,0.57,0.27)*disc*4.0}}
fn closest(scene:&Scene,r:Ray,tmax:f64)->Option<Hit>{let mut best=None;let mut limit=tmax;for b in &scene.boxes{if let Some(h)=b.hit(r,0.002,limit){limit=h.t;best=Some(h);if limit<0.01{break}}}best}
fn trace(scene:&Scene,r:Ray,depth:u32)->V{if depth>=MAX_DEPTH{return sky(r.d,scene.day)}let Some(h)=closest(scene,r,1e6)else{return sky(r.d,scene.day)};let mut n=h.n;if h.mat.normal_map{let q=V::new((h.p.x*8.0).sin()*0.055,(h.p.z*8.0).cos()*0.055,(h.p.x*5.0+h.p.z*6.0).sin()*0.055);n=(n+q).norm();if n.dot(h.n)<0.0{n=-n}}
 let base=texture(h.mat,h.p,n);let light=if scene.day{V::new(-0.4,0.9,-0.3).norm()}else{V::new(-0.55,0.8,-0.45).norm()};let shadow=closest(scene,Ray{o:h.p+n*0.006,d:light},120.0).is_some();let ambient=if scene.day{0.32}else{0.12};let diffuse=if shadow{ambient}else{n.dot(light).max(0.0)*if scene.day{0.95}else{0.76}+ambient};let view=-r.d;let halfv=(light+view).norm();let spec=n.dot(halfv).max(0.0).powf(42.0)*h.mat.spec*if shadow{0.25}else{1.0};let mut col=base*diffuse+V::new(1.0,0.82,0.58)*spec+h.mat.emission;
 // Local glow around intentional emissive lamps
 for b in &scene.boxes{if b.mat.emission.max()>0.0{let center=(b.lo+b.hi)*0.5;let dist=(center-h.p).len();if dist<6.0{col=col+V::new(1.0,0.49,0.14)*(b.mat.emission.max()*0.07/(1.0+dist*dist))}}}
 if depth<MAX_DEPTH-1&&(h.mat.refl>0.03||h.mat.trans>0.1){let reflected=trace(scene,Ray{o:h.p+n*0.008,d:r.d.reflect(n).norm()},depth+1);let mut amount=h.mat.refl;if h.mat.trans>0.0{let f0=((1.0-h.mat.ior)/(1.0+h.mat.ior)).powi(2);let cos=(-r.d).dot(n).max(0.0);let fres=f0+(1.0-f0)*(1.0-cos).powi(5);amount=amount.max(fres).max(h.mat.refl)}let mut mixed=col*(1.0-amount)+reflected*amount;if h.mat.trans>0.0{if let Some(rd)=r.d.refract(n,h.mat.ior){let refr=trace(scene,Ray{o:h.p-rd*0.008,d:rd.norm()},depth+1);mixed=mixed*(1.0-h.mat.trans)+refr.had(base)*h.mat.trans}}col=mixed}
 col}
fn camera_ray(scene:&Scene,u:f64,v:f64,zoom:f64)->Ray{let center=V::new(0.0,3.0,0.0);let radius=29.0/zoom;let angle=scene.spin;let eye=center+V::new(radius*angle.sin(),radius*0.68,radius*angle.cos());let forward=(center-eye).norm();let right=forward.cross(V::new(0.0,1.0,0.0)).norm();let up=right.cross(forward).norm();let aspect=W as f64/H as f64;let dir=(forward+right*((2.0*u-1.0)*aspect*0.37)+up*((1.0-2.0*v)*0.37)).norm();Ray{o:eye,d:dir}}

fn render_size(scene:Arc<Scene>,zoom:f64,workers:usize,w:usize,h:usize,fast:bool)->Vec<u32>{let rows_per=(h+workers-1)/workers;let mut joins=Vec::new();for start in (0..h).step_by(rows_per){let end=(start+rows_per).min(h);let s=Arc::clone(&scene);joins.push(thread::spawn(move||{let mut data=Vec::with_capacity((end-start)*w);for y in start..end{for x in 0..w{let ray=camera_ray(&s,(x as f64+0.5)/w as f64,(y as f64+0.5)/h as f64,zoom);let c=if fast{trace_fast(&s,ray)}else{trace(&s,ray,0)};let mapped=V::new(c.x/(1.0+c.x),c.y/(1.0+c.y),c.z/(1.0+c.z)).clamp();let gamma=|q:f64|((q.powf(1.0/2.2)*255.0)+0.5) as u32;data.push((gamma(mapped.x)<<16)|(gamma(mapped.y)<<8)|gamma(mapped.z));}}(start,data)}));}let mut pixels=vec![0u32;w*h];for j in joins{let (start,data)=j.join().expect("worker panicked");pixels[start*w..start*w+data.len()].copy_from_slice(&data)}pixels}

fn render(scene:Arc<Scene>,zoom:f64,workers:usize)->Vec<u32>{render_size(scene,zoom,workers,W,H,false)}

fn trace_fast(scene:&Scene,r:Ray)->V{let Some(h)=closest(scene,r,1e6)else{return sky(r.d,scene.day)};let n=h.n;let base=texture(h.mat,h.p,n);let light=if scene.day{V::new(-0.4,0.9,-0.3).norm()}else{V::new(-0.55,0.8,-0.45).norm()};let diffuse=n.dot(light).max(0.0)*if scene.day{0.95}else{0.70}+if scene.day{0.32}else{0.22};let view=-r.d;let halfv=(light+view).norm();let spec=n.dot(halfv).max(0.0).powf(30.0)*h.mat.spec;base*diffuse+V::new(1.0,0.82,0.58)*spec+h.mat.emission}

fn save_ppm(path:&str,pixels:&[u32]){let mut f=BufWriter::new(File::create(path).expect("no se pudo crear el archivo"));write!(f,"P6\n{} {}\n255\n",W,H).unwrap();for p in pixels{f.write_all(&[((p>>16)&255) as u8,((p>>8)&255) as u8,(p&255) as u8]).unwrap();}}

fn draw_button(p:&mut[u32],x0:usize,y0:usize,w:usize,h:usize,label:&str,color:u32){for y in y0..(y0+h).min(H){for x in x0..(x0+w).min(W){p[y*W+x]=color}}let scale=2;let text_w=label.len()*4*scale;let mut x=x0+(w.saturating_sub(text_w))/2;let y=y0+(h-5*scale)/2;for ch in label.bytes(){let glyph=match ch{b'A'=>[14,17,17,31,17],b'B'=>[30,17,30,17,30],b'C'=>[14,17,16,17,14],b'D'=>[30,17,17,17,30],b'E'=>[31,16,30,16,31],b'I'=>[31,4,4,4,31],b'L'=>[16,16,16,16,31],b'N'=>[17,25,21,19,17],b'O'=>[14,17,17,17,14],b'R'=>[30,17,30,20,18],b'S'=>[15,16,14,1,30],b'V'=>[17,17,17,10,4],b' '=>[0;5],_=>[0;5]};for (row,bits) in glyph.iter().enumerate(){for col in 0..5{if bits&(1<<(4-col))!=0{for dy in 0..scale{for dx in 0..scale{let xx=x+col*scale+dx;let yy=y+row*scale+dy;if xx<W&&yy<H{p[yy*W+xx]=0xFFFFFF}}}}}}x+=6*scale;}}

fn draw_ui(p:&mut[u32],day:bool,normal:bool,seed:u64){draw_button(p,8,8,125,24,if day{"DIA  C"}else{"NOCHE C"},0x35516F);draw_button(p,143,8,185,24,if normal{"NORMAL"}else{"BAJOS RECURSOS"},0x35516F);let label=format!("SEMILLA {seed}");draw_button(p,336,8,136,24,&label,0x35516F);}

fn interactive(){let workers=thread::available_parallelism().map(|n|n.get()).unwrap_or(1).min(H);let mut spin=0.65;let mut zoom=1.0;let mut day=false;let mut normal=true;let mut seed=1u64;let mut window=Window::new("Diorama nocturno — botones o C, arrastra para rotar, rueda para zoom, Esc para salir",W,H,WindowOptions{resize:true,scale:Scale::X1,..WindowOptions::default()}).expect("no se pudo abrir la ventana gráfica");window.set_target_fps(60);let mut pixels=vec![0u32;W*H];let mut dirty=true;let mut dragging=false;let mut moved=false;let mut last_x=0.0f32;let mut last_y=0.0f32;let mut last_scroll=0.0f32;let mut full_job:Option<(f64,f64,bool,thread::JoinHandle<Vec<u32>>)>=None;println!("Arrastra para rotar; rueda para zoom; C cambia día/noche; R genera otra isla; S guarda; Esc sale.");while window.is_open()&&!window.is_key_down(Key::Escape){if window.is_key_pressed(Key::C,minifb::KeyRepeat::No){day=!day;dirty=true}if window.is_key_pressed(Key::R,minifb::KeyRepeat::No){seed=seed.wrapping_add(1);dirty=true;println!("Nueva isla, semilla {seed}")}if window.is_key_pressed(Key::S,minifb::KeyRepeat::No){let full=render(Arc::new(build_scene(spin,day,seed)),zoom,workers);save_ppm("diorama.ppm",&full);println!("Guardado diorama.ppm")}let (mx,my)=window.get_mouse_pos(MouseMode::Clamp).unwrap_or((last_x,last_y));let pressed=window.get_mouse_down(MouseButton::Left);if pressed&&!dragging&&my<36.0{if mx<138.0{day=!day;dirty=true}else if mx<336.0{normal=!normal;dirty=true}dragging=false}else if pressed{if dragging{let dx=mx-last_x;let dy=my-last_y;spin-=dx as f64*0.012;zoom=(zoom*(dy as f64*0.012).exp()).clamp(0.65,1.8);if dx!=0.0||dy!=0.0{dirty=true;moved=true}}dragging=true}else{if dragging&&moved{moved=false;if normal&&full_job.is_none(){let s=Arc::new(build_scene(spin,day,seed));let z=zoom;full_job=Some((spin,zoom,day,thread::spawn(move||render(s,z,workers))))}}dragging=false}last_x=mx;last_y=my;let scroll=window.get_scroll_wheel().map(|s|s.1).unwrap_or(0.0);if scroll!=last_scroll{zoom=(zoom*((scroll-last_scroll) as f64*0.12).exp()).clamp(0.65,1.8);last_scroll=scroll;dirty=true}if dirty{let scene=Arc::new(build_scene(spin,day,seed));let mut frame=if normal{render(scene,zoom,workers)}else{let preview=render_size(scene,zoom,workers,INTERACTIVE_W,INTERACTIVE_H,true);let mut up=vec![0u32;W*H];for y in 0..H{for x in 0..W{up[y*W+x]=preview[(y/4)*INTERACTIVE_W+x/4]}}up};draw_ui(&mut frame,day,normal,seed);pixels=frame;dirty=false;if full_job.is_some(){full_job=None}}if let Some((job_spin,job_zoom,job_day,job))=full_job.take(){if job.is_finished(){if let Ok(full)=job.join(){if (job_spin-spin).abs()<1e-6&&(job_zoom-zoom).abs()<1e-6&&job_day==day&&normal{pixels=full;draw_ui(&mut pixels,day,normal,seed)}}}else{full_job=Some((job_spin,job_zoom,job_day,job))}}window.update_with_buffer(&pixels,W,H).expect("no se pudo actualizar la ventana")}}

fn main(){let args:Vec<String>=env::args().collect();if args.get(1).map(String::as_str)==Some("--interactive")||args.get(1).map(String::as_str)==Some("-i"){interactive();return}let spin=args.get(1).and_then(|v|v.parse().ok()).unwrap_or(0.0);let zoom=args.get(2).and_then(|v|v.parse::<f64>().ok()).unwrap_or(1.0).clamp(0.65,1.8);let out=args.get(3).map(String::as_str).unwrap_or("diorama.ppm");let scene=Arc::new(build_scene(spin,false,1));let workers=thread::available_parallelism().map(|n|n.get()).unwrap_or(1).min(H);let pixels=render(Arc::clone(&scene),zoom,workers);save_ppm(out,&pixels);println!("Render {out}: {W}x{H}, {} cubos, {workers} hilos; giro {spin:.2} rad, zoom {zoom:.2}x",scene.boxes.len());}
