import os
import time
import wave
import subprocess
import cv2
import numpy as np
from scipy.ndimage import gaussian_filter
from scipy.signal import butter, lfilter
import imageio_ffmpeg

W, H = 960, 540
FPS_SHORT = 24
FPS_FILM = 15

def build_fields(H=H, W=W):
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    sx_scale = W / 1408.0
    sy_scale = H / 768.0

    def sample_polyline(pts, n_samples=250):
        pts = np.array(pts, dtype=np.float32)
        pts[:, 0] *= sx_scale
        pts[:, 1] *= sy_scale
        pts[:, 3] *= 0.5 * (sx_scale + sy_scale)
        diffs = np.diff(pts[:, :2], axis=0)
        dists = np.hypot(diffs[:, 0], diffs[:, 1])
        cum = np.concatenate([[0], np.cumsum(dists)])
        total = cum[-1]
        s_grid = np.linspace(0, total, n_samples)
        interp = np.zeros((n_samples, 5), dtype=np.float32)
        for c in range(5):
            interp[:, c] = np.interp(s_grid, cum, pts[:, c])
        interp[:, 0] = gaussian_filter(interp[:, 0], sigma=1.5, mode='nearest')
        interp[:, 1] = gaussian_filter(interp[:, 1], sigma=1.5, mode='nearest')
        dx = np.gradient(interp[:, 0])
        dy = np.gradient(interp[:, 1])
        mag = np.hypot(dx, dy) + 1e-6
        tangents = np.stack([dx / mag, dy / mag], axis=1)
        return interp, tangents

    T_arrive = np.full((H, W), 99.0, dtype=np.float32)
    Vx = np.zeros((H, W), dtype=np.float32)
    Vy = np.zeros((H, W), dtype=np.float32)
    Tube_Mask = np.zeros((H, W), dtype=np.float32)
    Poiseuille = np.zeros((H, W), dtype=np.float32)

    def add_branch(pts_list, bbox_1408, poiseuille_lag=0.14):
        x0 = max(0, int(bbox_1408[0] * sx_scale))
        y0 = max(0, int(bbox_1408[1] * sy_scale))
        x1 = min(W, int(bbox_1408[2] * sx_scale))
        y1 = min(H, int(bbox_1408[3] * sy_scale))
        sub_x = xx[y0:y1, x0:x1][..., None]
        sub_y = yy[y0:y1, x0:x1][..., None]
        interp, tangents = sample_polyline(pts_list)
        d2 = (sub_x - interp[None, None, :, 0])**2 + (sub_y - interp[None, None, :, 1])**2
        idx = np.argmin(d2, axis=2)
        min_d = np.sqrt(np.take_along_axis(d2, idx[..., None], axis=2)[..., 0])
        t_val = interp[idx, 2]
        r_val = interp[idx, 3]
        spd_val = interp[idx, 4] * sx_scale
        tx = tangents[idx, 0]
        ty = tangents[idx, 1]

        norm_r = min_d / (r_val + 1e-5)
        w = np.clip(1.0 - (norm_r - 0.85) / 0.45, 0.0, 1.0)
        t_eff = t_val + poiseuille_lag * np.clip(norm_r**2, 0.0, 1.4)
        flow_profile = np.clip(1.0 - (norm_r / 1.15)**2, 0.0, 1.0)

        cur_T = T_arrive[y0:y1, x0:x1]
        update = (w > 0.01) & (t_eff < cur_T)
        cur_T[update] = t_eff[update]
        T_arrive[y0:y1, x0:x1] = cur_T

        cur_w = Tube_Mask[y0:y1, x0:x1]
        better = flow_profile * w > cur_w
        Vx[y0:y1, x0:x1] = np.where(better, tx * spd_val * flow_profile * w, Vx[y0:y1, x0:x1])
        Vy[y0:y1, x0:x1] = np.where(better, ty * spd_val * flow_profile * w, Vy[y0:y1, x0:x1])
        Tube_Mask[y0:y1, x0:x1] = np.maximum(cur_w, flow_profile * w)
        Poiseuille[y0:y1, x0:x1] = np.maximum(Poiseuille[y0:y1, x0:x1], w)

    add_branch([(432,60,0.2,18,40),(402,135,0.5,20,38),(393,215,0.78,34,36),(393,255,0.9,30,34)], (320,45,470,275), 0.06)
    add_branch([(393,252,0.88,32,30),(354,268,1.08,36,30),(320,340,1.35,36,30),(350,435,1.62,36,28),(425,440,1.82,34,28),(490,380,2.04,28,28)], (285,225,505,480))
    add_branch([(490,380,2.02,28,28),(515,290,2.28,38,28),(585,264,2.52,38,28),(676,272,2.78,28,28)], (475,225,685,420))
    add_branch([(490,380,2.02,30,28),(545,445,2.30,36,28),(635,420,2.56,36,28),(650,282,2.76,34,28)], (475,250,685,480))
    add_branch([(674,272,2.76,28,28),(722,268,2.92,36,28),(810,268,3.28,28,28)], (665,230,818,310))
    add_branch([(722,268,2.90,36,26),(735,195,3.15,36,24),(792,172,3.38,36,18)], (685,135,815,285))
    add_branch([(722,268,2.90,36,28),(724,365,3.20,34,24),(724,448,3.45,34,18)], (685,255,770,475))
    add_branch([(808,268,3.26,28,28),(850,268,3.42,36,28),(942,272,3.78,28,28)], (800,230,950,310))
    add_branch([(850,268,3.40,36,26),(865,195,3.65,36,24),(918,172,3.88,36,18)], (812,135,942,285))
    add_branch([(850,268,3.40,36,28),(852,365,3.70,34,24),(852,448,3.95,34,18)], (812,255,898,475))
    add_branch([(940,272,3.76,32,28),(1015,266,4.02,38,28),(1088,325,4.32,36,26),(980,348,4.65,32,24)], (920,230,1125,385))
    add_branch([(940,272,3.76,34,28),(950,375,4.12,40,28),(1028,456,4.62,38,28),(1078,422,4.80,36,22)], (918,250,1115,482))
    add_branch([(1028,456,4.62,24,38),(1029,495,4.88,18,46),(1030,528,5.10,18,48)], (990,448,1070,538), 0.04)

    cup_d = np.hypot((xx - 1030.0 * sx_scale) / (118.0 * sx_scale), (yy - 526.0 * sy_scale) / (16.0 * sy_scale))
    cup_mask = np.clip(1.0 - (cup_d - 0.92) / 0.25, 0.0, 1.0)
    cup_t = 5.10 + 0.50 * np.clip(cup_d, 0.0, 1.2)
    upd = (cup_mask > 0.01) & (cup_t < T_arrive)
    T_arrive[upd] = cup_t[upd]
    Poiseuille[upd] = np.maximum(Poiseuille[upd], cup_mask[upd])
    return T_arrive, Vx, Vy, Tube_Mask, Poiseuille

def generate_short_audio(filename='assets/coffee_pour_audio.wav', duration=10.0, sr=44100):
    n = int(duration * sr)
    t = np.linspace(0, duration, n, endpoint=False, dtype=np.float32)
    rng = np.random.RandomState(1234)
    nyq = 0.5 * sr
    def bp(sig, lo, hi):
        b, a = butter(2, [lo / nyq, hi / nyq], btype='band')
        return lfilter(b, a, sig).astype(np.float32)

    wL = rng.randn(n).astype(np.float32)
    wR = rng.randn(n).astype(np.float32)
    env_k = np.clip((t - 0.18) / 0.25, 0, 1) * np.clip((7.4 - t) / 0.35, 0, 1)
    kettle = (bp(wL, 380, 2400) * 0.065 + bp(wR, 2500, 6500) * 0.028) * env_k

    env_t = np.clip((t - 0.82) / 0.25, 0, 1) * np.clip((8.3 - t) / 0.9, 0, 1)
    tube = bp(wL + wR, 450, 1350) * 0.055 * (0.6 + 0.4 * np.sin(2 * np.pi * 6.5 * t)) * env_t

    env_c = np.clip((t - 4.78) / 0.22, 0, 1) * np.clip((8.7 - t) / 0.45, 0, 1)
    cup = (bp(wR, 500, 1800) * 0.07 + bp(wL, 1800, 4800) * 0.05) * env_c

    drip = np.zeros(n, dtype=np.float32)
    idx0 = int(9.12 * sr)
    dlen = int(0.09 * sr)
    tl = np.arange(dlen, dtype=np.float32) / sr
    f_inst = 780.0 * (1.0 + 0.85 * (1.0 - np.exp(-tl * 55.0)))
    ph = 2 * np.pi * np.cumsum(f_inst) / sr
    drip[idx0:idx0+dlen] = 0.15 * np.exp(-tl * 36.0) * np.sin(ph)

    pan = np.clip(0.3 + 0.42 * np.clip((t - 0.85) / 3.95, 0, 1), 0.25, 0.75)
    L = np.tanh((kettle * 0.68 + tube * (1 - pan) + cup * 0.4 + drip * 0.45) * 2.0) * 0.82
    R = np.tanh((kettle * 0.32 + tube * pan + cup * 0.6 + drip * 0.55) * 2.0) * 0.82
    pcm = (np.stack([L, R], axis=1) * 32767).astype(np.int16)
    with wave.open(filename, 'w') as wf:
        wf.setnchannels(2)
        wf.setsampwidth(2)
        wf.setframerate(sr)
        wf.writeframes(pcm.tobytes())

def synthesize_coffe_frames():
    base_filled = cv2.resize(cv2.imread('assets/ai_frame_1_empty.png'), (W, H), interpolation=cv2.INTER_AREA).astype(np.float32)
    T_arrive, Vx, Vy, Tube_Mask, Poiseuille = build_fields(H, W)
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)

    luma = 0.114 * base_filled[..., 0] + 0.587 * base_filled[..., 1] + 0.299 * base_filled[..., 2]
    glass_rim = np.clip((luma - 155.0) / 70.0, 0.0, 1.0)
    bg_warm = np.zeros_like(base_filled)
    bg_warm[..., 0] = 14.0
    bg_warm[..., 1] = 28.0
    bg_warm[..., 2] = 52.0
    in_tube_or_stream = np.clip(Poiseuille * 1.15, 0.0, 1.0) * np.clip((490.0 * (H / 768.0) - yy) / 10.0, 0.0, 1.0)
    empty_interior = bg_warm * (1.0 - glass_rim[..., None]) + base_filled * glass_rim[..., None]
    im_empty = base_filled * (1.0 - in_tube_or_stream[..., None] * 0.88) + empty_interior * (in_tube_or_stream[..., None] * 0.88)

    crema_col = np.array([45.0, 138.0, 222.0], dtype=np.float32)
    roast_col = np.array([10.0, 22.0, 48.0], dtype=np.float32)
    rng = np.random.RandomState(42)
    n_med = gaussian_filter(rng.randn(H, W).astype(np.float32), sigma=5.0)
    n_med /= (np.std(n_med) + 1e-6)

    fluid_bank = []
    opt_bank = []
    n_frames = 120
    for i in range(n_frames):
        t = (i / float(n_frames - 1)) * 10.0
        dt_pour = t - (T_arrive + 0.04 * n_med)
        reveal = np.clip((dt_pour + 0.04) / 0.12, 0.0, 1.0)
        reveal = reveal * reveal * (3.0 - 2.0 * reveal) * np.clip(Poiseuille * 1.15, 0.0, 1.0)

        wave_u = 13.0 * T_arrive - 11.0 * t + 0.85 * n_med
        swirl = 0.5 + 0.5 * np.sin(wave_u + np.cos(0.08 * xx - 6.0 * t))
        crema_ratio = np.clip(0.35 + 0.45 * swirl, 0.0, 1.0)[..., None]
        rich_liquid = crema_ratio * crema_col + (1.0 - crema_ratio) * roast_col
        filled_dynamic = 0.48 * base_filled + 0.52 * rich_liquid
        filled_dynamic = filled_dynamic * (1.0 - 0.65 * glass_rim[..., None]) + base_filled * (0.65 * glass_rim[..., None])

        front = np.exp(-((dt_pour - 0.09) / 0.09)**2) * reveal
        foam_col = np.array([78.0, 175.0, 246.0], dtype=np.float32)
        filled_dynamic = filled_dynamic * (1.0 - 0.65 * front[..., None]) + foam_col * (0.65 * front[..., None])

        frame = im_empty * (1.0 - reveal[..., None]) + filled_dynamic * reveal[..., None]

        if t > 7.2:
            w_set = np.clip((t - 7.2) / 1.4, 0.0, 1.0)
            w_set = w_set * w_set * (3.0 - 2.0 * w_set)
            frame = frame * (1.0 - w_set) + base_filled * w_set

        frame_u8 = np.clip(frame, 0, 255).astype(np.uint8)
        fluid_bank.append(frame_u8)

        u = i / float(n_frames - 1)
        pan_w = np.sin(np.clip((u - 0.08) / 0.72, 0.0, 1.0) * np.pi)
        cx = W * 0.5 + pan_w * ((u - 0.42) * W * 0.22)
        cy = H * 0.48 + pan_w * (H * 0.04)
        zoom = 1.0 + 0.28 * pan_w
        M = cv2.getRotationMatrix2D((float(cx), float(cy)), 0.0, float(zoom))
        opt_bank.append(cv2.warpAffine(frame_u8, M, (W, H), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT))

    return fluid_bank, opt_bank

def save_short_video(bank, audio_path, out_path, duration=10.0):
    ffmpeg = imageio_ffmpeg.get_ffmpeg_exe()
    n_out = int(duration * FPS_SHORT)
    cmd = [
        ffmpeg, '-y',
        '-f', 'rawvideo', '-vcodec', 'rawvideo',
        '-s', f'{W}x{H}', '-pix_fmt', 'bgr24', '-r', str(FPS_SHORT),
        '-i', '-', '-i', audio_path,
        '-c:v', 'libx264', '-preset', 'fast', '-crf', '21', '-pix_fmt', 'yuv420p',
        '-c:a', 'aac', '-b:a', '128k', '-shortest', '-movflags', '+faststart',
        out_path
    ]
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
    for i in range(n_out):
        pos = (i / float(n_out - 1)) * (len(bank) - 1)
        i0 = int(pos)
        i1 = min(len(bank) - 1, i0 + 1)
        f = cv2.addWeighted(bank[i0], 1.0 - (pos - i0), bank[i1], pos - i0, 0.0)
        p.stdin.write(f.tobytes())
    p.stdin.close()
    p.wait()
    print(f"Saved {out_path} ({os.path.getsize(out_path)/1e6:.2f} MB)")

def build_5min_feature(fluid_bank, opt_bank):
    ffmpeg = imageio_ffmpeg.get_ffmpeg_exe()
    sr = 44100
    duration = 300.0
    N = int(duration * sr)
    t_aud = np.linspace(0, duration, N, endpoint=False, dtype=np.float32)

    voice_L = np.zeros(N, dtype=np.float32)
    voice_R = np.zeros(N, dtype=np.float32)
    voice_env = np.zeros(N, dtype=np.float32)
    act_starts = [4.0, 63.0, 123.0, 183.0, 243.0]
    for i, st in enumerate(act_starts, start=1):
        res = subprocess.run([
            ffmpeg, '-y', '-i', f'assets/narration_act{i}.mp3',
            '-f', 's16le', '-acodec', 'pcm_s16le', '-ar', str(sr), '-ac', '1', '-'
        ], capture_output=True)
        clip = np.frombuffer(res.stdout, dtype=np.int16).astype(np.float32) / 32768.0
        idx0 = int(st * sr)
        idx1 = min(N, idx0 + len(clip))
        voice_L[idx0:idx1] += clip[:idx1-idx0] * 0.92
        voice_R[idx0:idx1] += clip[:idx1-idx0] * 0.92
        voice_env[max(0, idx0 - int(0.5*sr)):min(N, idx1 + int(0.5*sr))] = 1.0

    music_gain = 1.0 - 0.50 * voice_env
    score_L = (
        0.045 * np.sin(2 * np.pi * 146.83 * t_aud)
        + 0.035 * np.sin(2 * np.pi * 220.00 * t_aud + 0.4 * np.sin(2 * np.pi * 0.2 * t_aud))
        + 0.028 * np.sin(2 * np.pi * 293.66 * t_aud)
        + 0.022 * np.sin(2 * np.pi * 369.99 * t_aud + 0.5 * np.cos(2 * np.pi * 0.15 * t_aud))
    ) * music_gain
    score_R = (
        0.045 * np.sin(2 * np.pi * 146.95 * t_aud)
        + 0.035 * np.sin(2 * np.pi * 220.18 * t_aud - 0.4 * np.sin(2 * np.pi * 0.2 * t_aud))
        + 0.028 * np.sin(2 * np.pi * 293.80 * t_aud)
        + 0.022 * np.sin(2 * np.pi * 370.20 * t_aud)
    ) * music_gain

    wav_5min = 'assets/temp_5min_audio.wav'
    mix_L = np.tanh((voice_L * 1.15 + score_L) * 1.35) * 0.88
    mix_R = np.tanh((voice_R * 1.15 + score_R) * 1.35) * 0.88
    pcm = (np.stack([mix_L, mix_R], axis=1) * 32767.0).astype(np.int16)
    with wave.open(wav_5min, 'w') as wf:
        wf.setnchannels(2)
        wf.setsampwidth(2)
        wf.setframerate(sr)
        wf.writeframes(pcm.tobytes())

    p_neb = cv2.resize(cv2.imread('assets/cinema_act1_nebula.png'), (W, H), interpolation=cv2.INTER_AREA)
    p_mtn = cv2.resize(cv2.imread('assets/cinema_act1_mountains.png'), (W, H), interpolation=cv2.INTER_AREA)
    p_roa = cv2.resize(cv2.imread('assets/cinema_act2_roastery.png'), (W, H), interpolation=cv2.INTER_AREA)
    p_alc = cv2.resize(cv2.imread('assets/cinema_act2_alchemist.png'), (W, H), interpolation=cv2.INTER_AREA)
    p_oce = cv2.resize(cv2.imread('assets/cinema_act4_micro_ocean.png'), (W, H), interpolation=cv2.INTER_AREA)
    p_caf = cv2.resize(cv2.imread('assets/cinema_act5_rain_cafe.png'), (W, H), interpolation=cv2.INTER_AREA)

    out_film = 'cinema_5min_feature_film.mp4'
    total_frames = int(duration * FPS_FILM)
    cmd = [
        ffmpeg, '-y',
        '-f', 'rawvideo', '-vcodec', 'rawvideo',
        '-s', f'{W}x{H}', '-pix_fmt', 'bgr24', '-r', str(FPS_FILM),
        '-i', '-', '-i', wav_5min,
        '-c:v', 'libx264', '-preset', 'faster', '-crf', '26',
        '-maxrate', '600k', '-bufsize', '1200k', '-pix_fmt', 'yuv420p',
        '-c:a', 'aac', '-b:a', '128k', '-shortest', '-movflags', '+faststart',
        out_film
    ]
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
    rng = np.random.RandomState(77)
    pt_x0 = rng.uniform(0, W, 60).astype(np.float32)
    pt_y0 = rng.uniform(0, H, 60).astype(np.float32)
    pt_vx = rng.uniform(-22, 22, 60).astype(np.float32)
    pt_vy = rng.uniform(-35, -10, 60).astype(np.float32)

    for idx in range(total_frames):
        t = idx / float(FPS_FILM)
        if t < 30.0:
            zoom = 1.0 + 0.14 * (t / 30.0)
            ang = 1.8 * np.sin(t * 0.15)
            M = cv2.getRotationMatrix2D((W*0.5, H*0.5), float(ang), float(zoom))
            frame = cv2.warpAffine(p_neb, M, (W, H), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
        elif t < 60.0:
            tl = (t - 30.0) / 30.0
            M = cv2.getRotationMatrix2D((W*(0.46 + 0.08*tl), H*0.5), 0.0, float(1.12 - 0.10*tl))
            frame = cv2.warpAffine(p_mtn, M, (W, H), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
        elif t < 90.0:
            tl = (t - 60.0) / 30.0
            M = cv2.getRotationMatrix2D((W*(0.52 - 0.05*tl), H*0.48), 0.0, float(1.02 + 0.12*tl))
            frame = cv2.warpAffine(p_roa, M, (W, H), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
        elif t < 120.0:
            tl = (t - 90.0) / 30.0
            M = cv2.getRotationMatrix2D((W*0.48, H*0.52), 0.0, float(1.0 + 0.14*tl))
            frame = cv2.warpAffine(p_alc, M, (W, H), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
        elif t < 180.0:
            tl = t - 120.0
            if tl < 30.0:
                u_p = tl / 30.0
                b_idx = min(len(fluid_bank)-1, int(u_p * (len(fluid_bank)-1)))
                frame = fluid_bank[b_idx].copy()
            else:
                u_p = (tl - 30.0) / 30.0
                b_idx = min(len(opt_bank)-1, int(u_p * (len(opt_bank)-1)))
                frame = opt_bank[b_idx].copy()
        elif t < 240.0:
            tl = (t - 180.0) / 60.0
            zoom = 1.0 + 0.15 * np.sin(tl * np.pi)
            ang = 2.5 * np.sin(tl * 2.0 * np.pi)
            M = cv2.getRotationMatrix2D((W*0.58, H*0.44), float(ang), float(zoom))
            frame = cv2.warpAffine(p_oce, M, (W, H), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
        else:
            tl = (t - 240.0) / 60.0
            M = cv2.getRotationMatrix2D((W*0.55, H*0.55), 0.0, float(1.10 - 0.09*tl))
            frame = cv2.warpAffine(p_caf, M, (W, H), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)

        px = ((pt_x0 + pt_vx * t) % W).astype(np.int32)
        py = ((pt_y0 + pt_vy * t) % H).astype(np.int32)
        for k in range(0, 60, 2):
            cv2.circle(frame, (int(px[k]), int(py[k])), 2, (95, 195, 255), -1, lineType=cv2.LINE_AA)

        p.stdin.write(frame.tobytes())

    p.stdin.close()
    p.wait()
    os.remove(wav_5min)
    print(f"Saved {out_film} ({os.path.getsize(out_film)/1e6:.2f} MB)")

if __name__ == '__main__':
    t0 = time.time()
    generate_short_audio('assets/coffee_pour_audio.wav')
    fluid_bank, opt_bank = synthesize_coffe_frames()
    save_short_video(fluid_bank, 'assets/coffee_pour_audio.wav', 'coffe_pouring_video.mp4')
    save_short_video(opt_bank, 'assets/coffee_pour_audio.wav', 'coffe_optical_flow_cut.mp4')
    build_5min_feature(fluid_bank, opt_bank)
    os.remove('assets/coffee_pour_audio.wav')
    print(f"All videos built in {time.time() - t0:.1f}s")
