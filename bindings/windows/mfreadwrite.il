/// mfreadwrite.dll — Media Foundation Source Reader + Sink Writer.
///
/// `MFCreateSourceReaderFromURL` is the modern, easy way to
/// decode video / audio in Media Foundation:
///
///   let reader: *void = 0 as *void
///   MFCreateSourceReaderFromURL(L"video.mp4", attrs, &reader)
///   loop {
///       reader.ReadSample(streamIdx, 0, &actualStream, &flags,
///                         &timestamp, &sample)
///       ...
///   }
///
/// COM methods on `IMFSourceReader` / `IMFSinkWriter` aren't yet
/// bound here.

@extern(C, "mfreadwrite") {
    /// Create a reader bound to a URL. `pAttributes` is an
    /// `IMFAttributes` (may be 0) for hints like
    /// MF_SOURCE_READER_ENABLE_HARDWARE_TRANSFORMS.
    @lib pub fn MFCreateSourceReaderFromURL(
        pwszURL:        *const u16,
        pAttributes:    *void,
        ppSourceReader: **void
    ): i32

    /// Create a reader over an existing `IMFByteStream`.
    @lib pub fn MFCreateSourceReaderFromByteStream(
        pByteStream:    *void,
        pAttributes:    *void,
        ppSourceReader: **void
    ): i32

    /// Create a reader from an existing `IMFMediaSource`.
    @lib pub fn MFCreateSourceReaderFromMediaSource(
        pMediaSource:   *void,
        pAttributes:    *void,
        ppSourceReader: **void
    ): i32

    /// Create a writer for `pwszOutputURL` (file path / .mp4 etc.).
    @lib pub fn MFCreateSinkWriterFromURL(
        pwszOutputURL:  *const u16,
        pByteStream:    *void,
        pAttributes:    *void,
        ppSinkWriter:   **void
    ): i32

    /// Create a writer over an existing `IMFMediaSink`.
    @lib pub fn MFCreateSinkWriterFromMediaSink(
        pMediaSink:     *void,
        pAttributes:    *void,
        ppSinkWriter:   **void
    ): i32
}
