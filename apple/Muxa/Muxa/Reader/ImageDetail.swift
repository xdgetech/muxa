import MuxaProtocol
import MuxaSession
import SwiftUI
import UIKit

/// The open image at full size, with pinch zoom, and Previous and Next through
/// the images of its folder.
struct ImageDetail: View {
    @Environment(AppModel.self) private var model
    let path: String
    @Binding var selection: ReaderItem?

    var body: some View {
        content
            .navigationTitle(ReaderScreen.name(path))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItemGroup(placement: .primaryAction) {
                    Button { step(-1) } label: { Image(systemName: "chevron.left") }
                        .keyboardShortcut(.leftArrow, modifiers: [])
                        .disabled(index.map { $0 == 0 } ?? true)
                        .accessibilityLabel("Previous Image")
                        .accessibilityIdentifier("image-previous")
                    if let index {
                        Text("\(index + 1) of \(siblings.count)")
                            .font(.callout.monospacedDigit())
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("image-position")
                    }
                    Button { step(1) } label: { Image(systemName: "chevron.right") }
                        .keyboardShortcut(.rightArrow, modifiers: [])
                        .disabled(index.map { $0 + 1 >= siblings.count } ?? true)
                        .accessibilityLabel("Next Image")
                        .accessibilityIdentifier("image-next")
                }
            }
    }

    @ViewBuilder
    private var content: some View {
        if let image = model.session.image, image.path == path {
            if let data = image.data {
                ZoomableImage(data: data)
                    .ignoresSafeArea(edges: .bottom)
                    .accessibilityIdentifier("image-view")
                    .overlay(alignment: .bottom) {
                        if let ended = image.ended { EndedNotice(text: ended) }
                    }
            } else if let ended = image.ended {
                ContentUnavailableView("Can't Open \(ReaderScreen.name(path))", systemImage: Icon.warning,
                                       description: Text(ended))
            } else {
                ProgressView()
            }
        } else {
            ProgressView()
        }
    }

    private var siblings: [ImageInfo] {
        ImageFolder.siblings(of: path, in: model.imageListing?.entries ?? [])
    }

    private var index: Int? { siblings.firstIndex { $0.path == path } }

    private func step(_ by: Int) {
        guard let index, siblings.indices.contains(index + by) else { return }
        selection = .image(siblings[index + by].path)
    }
}

/// A `UIScrollView` zoom: fits the image at first, pinch and double-tap zoom in.
/// When the same-sized image is rewritten, the zoom and position stay.
struct ZoomableImage: UIViewRepresentable {
    let data: Data

    func makeUIView(context: Context) -> ZoomView { ZoomView() }

    func updateUIView(_ view: ZoomView, context: Context) { view.show(data) }
}

final class ZoomView: UIScrollView, UIScrollViewDelegate {
    private let imageView = UIImageView()
    private let unsupported = UILabel()
    /// Decoded only when the bytes change: a screenshot can be megabytes.
    private var shown: Data?
    /// The fit scale was set for this view size and image size.
    private var fitted: (CGSize, CGSize)?

    init() {
        super.init(frame: .zero)
        delegate = self
        showsHorizontalScrollIndicator = false
        showsVerticalScrollIndicator = false
        contentInsetAdjustmentBehavior = .never
        backgroundColor = .systemBackground
        imageView.contentMode = .scaleAspectFit
        addSubview(imageView)
        unsupported.text = String(localized: "This image format is not supported.")
        unsupported.textColor = .secondaryLabel
        unsupported.isHidden = true
        addSubview(unsupported)
        let doubleTap = UITapGestureRecognizer(target: self, action: #selector(toggleZoom(_:)))
        doubleTap.numberOfTapsRequired = 2
        addGestureRecognizer(doubleTap)
    }

    required init?(coder: NSCoder) { fatalError("not used from a storyboard") }

    func show(_ data: Data) {
        guard data != shown else { return }
        shown = data
        let image = UIImage(data: data)
        unsupported.isHidden = image != nil
        let sameSize = imageView.image?.size == image?.size
        imageView.image = image
        if !sameSize {
            fitted = nil
            setNeedsLayout()
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        unsupported.sizeToFit()
        unsupported.center = CGPoint(x: bounds.midX, y: bounds.midY)
        guard let size = imageView.image?.size, size.width > 0, size.height > 0, bounds.width > 0 else { return }
        if fitted.map({ $0.0 != bounds.size || $0.1 != size }) ?? true {
            fitted = (bounds.size, size)
            imageView.frame = CGRect(origin: .zero, size: size)
            contentSize = size
            let fit = min(bounds.width / size.width, bounds.height / size.height, 1)
            minimumZoomScale = fit
            maximumZoomScale = max(fit * 8, 2)
            zoomScale = fit
        }
        center()
    }

    func viewForZooming(in scrollView: UIScrollView) -> UIView? { imageView }

    func scrollViewDidZoom(_ scrollView: UIScrollView) { center() }

    /// Keeps a smaller-than-the-view image in the middle.
    private func center() {
        let x = max((bounds.width - contentSize.width) / 2, 0)
        let y = max((bounds.height - contentSize.height) / 2, 0)
        contentInset = UIEdgeInsets(top: y, left: x, bottom: y, right: x)
    }

    @objc private func toggleZoom(_ tap: UITapGestureRecognizer) {
        if zoomScale > minimumZoomScale {
            setZoomScale(minimumZoomScale, animated: true)
        } else {
            let point = tap.location(in: imageView)
            let scale = min(max(minimumZoomScale * 2.5, 1), maximumZoomScale)
            let size = CGSize(width: bounds.width / scale, height: bounds.height / scale)
            zoom(to: CGRect(x: point.x - size.width / 2, y: point.y - size.height / 2,
                            width: size.width, height: size.height), animated: true)
        }
    }
}
